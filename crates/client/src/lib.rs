//! Client web (WASM) de Cairn : une fenêtre sur le monde **vivant**.
//!
//! Le client fait tourner un [`Sim`] localement (Phase 2 : pas encore de
//! serveur — celui-ci arrive en Phase 6, où la sim déménagera côté serveur et
//! le client s'abonnera à sa région). Chaque frame : on avance la simulation
//! d'un peu de temps de jeu, on peint le terrain, puis les agents et la faune
//! par-dessus.
//!
//! Choix de rendu : le **terrain** échantillonne le worldgen (baseline pur) et
//! ne change qu'au pan/zoom — on le met donc en cache et on ne le recalcule
//! qu'à l'invalidation. Les **entités**, elles, bougent à chaque tick : on les
//! redessine chaque frame au-dessus du terrain caché, en appels canvas 2D
//! (petits carrés pixel-art), ce qui est bien plus léger que de les graver
//! dans le tampon.

// L'initialiseur du thread_local est déjà `const` ; ce lint le signale à tort.
#![allow(clippy::missing_const_for_thread_local)]

pub mod palette;
pub mod render;

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use cairn_core::WorldSeed;
use cairn_core::scale::{km_to_tiles, tiles_to_km};
use cairn_sim::{
    Activity, AgentId, Behavior, DeathCause, Demographics, Exposure, Exposures, FaunaId, Herd,
    Kinship, Knowledge, Memory, Pack, Physiology, Position, Sex, Sim, Skills, StructureKind,
    TaskKind, Traits,
};
use cairn_worldgen::{HumidityConfig, WorldGenConfig};
use wasm_bindgen::prelude::*;
use wasm_bindgen::{Clamped, JsCast};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

use palette::Layer;

/// Zoom minimal et maximal, en pixels par tuile.
const MIN_SCALE: f64 = 0.0004;
const MAX_SCALE: f64 = 32.0;
/// Zoom maximal du mode « Suivre » : quand la population est très groupée, on
/// ne zoome pas au-delà (sinon on collerait à un seul agent).
const FOLLOW_MAX_SCALE: f64 = 3.0;
/// Rayon (pixels) autour d'un clic dans lequel on capte un humain à inspecter.
const PICK_RADIUS_PX: f64 = 16.0;

/// Capacité du store de chunks résidents (l'éviction bornée protège la
/// mémoire au-delà). Modeste : la scène du client est locale.
const CHUNK_CAPACITY: usize = 2048;
/// Population humaine de départ.
const START_AGENTS: usize = 40;
/// Points conservés dans l'historique de population (un par jour de jeu
/// échantillonné) : ~400 jours, largement assez pour « voir l'évolution »
/// sans grossir indéfiniment.
const POP_HISTORY_CAP: usize = 400;
/// Dimensions du petit graphique d'évolution du panneau Population — mêmes
/// valeurs que les attributs `width`/`height` du `<canvas>` dans le HTML.
const POP_CHART_W: f64 = 230.0;
const POP_CHART_H: f64 = 56.0;

/// Paliers de vitesse proposés, en **ticks de jeu par seconde réelle**. Un
/// tick = une heure ; 24 ticks/s = un jour de jeu par seconde. Le plus lent
/// (1,5) = un jour toutes les ~16 s, pour suivre un agent pas à pas
/// (l'interpolation le fait glisser, d'autant plus lisse que c'est lent).
const SPEEDS: [f64; 4] = [1.5, 6.0, 24.0, 96.0];
/// Plafond de ticks simulés par frame : après un onglet en arrière-plan, on ne
/// rattrape pas des heures de jeu d'un coup (ça figerait la page).
const MAX_TICKS_PER_FRAME: u32 = 24;

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let mut app = App::new()?;
    // Hook `?ticks=N` : pré-avance la simulation avant le premier rendu. Sert
    // à démarrer « plus tard » et à vérifier le moteur hors navigateur
    // interactif (une capture montre alors un monde qui a réellement tourné).
    if let Some(n) = query_param_u64("ticks") {
        for _ in 0..n {
            app.sim.step();
            app.sample_population();
        }
    }
    // Hook `?select=<id>` : pré-sélectionne un agent — pratique à l'usage, et
    // surtout ce qui rend le panneau d'agent **vérifiable en headless** (un
    // `--dump-dom` montre alors le panneau rempli, sans simuler de clic).
    if let Some(sel) = query_param_u64("select") {
        app.selected = Some(sel);
    }
    APP.with(|slot| *slot.borrow_mut() = Some(app));
    install_event_handlers()?;
    // Première frame **synchrone** (cadrée sur la population) : le monde
    // s'affiche dès le chargement, sans flash noir en attendant le premier
    // `requestAnimationFrame`.
    with_app(|a| {
        if a.selected.is_some() {
            activate_tab("agent");
        }
        a.follow_population();
        a.render();
    });
    // Mode `?static=1` : on ne lance PAS la boucle — une seule frame figée.
    // Utile pour une capture reproductible (le temps virtuel headless se
    // stabilise, la boucle rAF permanente l'empêcherait).
    if query_param_u64("static").is_none() {
        schedule_frame();
    }
    Ok(())
}

struct Camera {
    cx: f64,
    cy: f64,
    scale: f64,
}

/// Trois façons de colorer les humains : couleur fixe, activité en cours, ou
/// sexe/âge — le portrait démographique de la population en un coup d'œil,
/// sans ouvrir le panneau. Bouclé par clics successifs sur le même bouton.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorMode {
    Fixed,
    Activity,
    SexAge,
}

impl ColorMode {
    fn next(self) -> Self {
        match self {
            ColorMode::Fixed => ColorMode::Activity,
            ColorMode::Activity => ColorMode::SexAge,
            ColorMode::SexAge => ColorMode::Fixed,
        }
    }

    fn label(self) -> &'static str {
        match self {
            ColorMode::Fixed => "Couleur : humain",
            ColorMode::Activity => "Couleur : activité",
            ColorMode::SexAge => "Couleur : sexe/âge",
        }
    }
}

struct App {
    sim: Sim,
    ctx: CanvasRenderingContext2d,
    canvas: HtmlCanvasElement,
    /// Contexte du petit graphique d'évolution démographique (onglet
    /// Population) — un second canvas, minuscule et de taille fixe, donc pas
    /// besoin de le redimensionner comme la vue principale.
    pop_chart_ctx: CanvasRenderingContext2d,
    width: usize,
    height: usize,
    camera: Camera,
    layer: Layer,
    drag: Option<(f64, f64)>,
    /// Pixel du dernier `mousedown`, pour distinguer un **clic** (placement)
    /// d'un **glisser** (pan) au relâchement.
    press: Option<(f64, f64)>,

    // — Boucle temporelle —
    playing: bool,
    /// Vitesse en ticks de jeu par seconde réelle.
    speed: f64,
    /// Accumulateur de ticks fractionnaires entre deux frames.
    tick_acc: f64,
    /// Horodatage de la frame précédente (ms), ou `None` à la première.
    last_ts: Option<f64>,

    /// Seed courante (pour l'affichage et la régénération).
    seed: u64,
    /// Mode « poser des humains au clic » armé.
    placing: bool,
    /// Agent inspecté (son `AgentId`), sélectionné au clic. `None` = aucun.
    /// Purement d'affichage — la simulation l'ignore.
    selected: Option<u64>,
    /// La caméra suit le barycentre de la population (recadrage à seuil).
    follow: bool,
    /// Comment les humains sont colorés (activité, sexe/âge, ou fixe).
    color_mode: ColorMode,

    // — Suivi démographique (onglet Population) —
    /// Population échantillonnée une fois par jour de jeu : de quoi tracer
    /// une courbe d'évolution sans stocker un point par tick.
    pop_history: VecDeque<(u64, u32)>,
    /// Dernier jour échantillonné, pour ne pousser qu'un point par jour.
    last_sampled_day: Option<u64>,

    // — Interpolation d'affichage —
    /// Position de chaque entité **au tick précédent**, par identifiant stable
    /// (humains et faune). Sert à interpoler le rendu entre deux ticks pour un
    /// mouvement fluide (le brief §8.5 : « le client interpole »). Rendu
    /// uniquement — pas de la simulation —, d'où une `HashMap` sans souci de
    /// déterminisme.
    prev_pos: HashMap<u64, (f64, f64)>,
    prev_fauna: HashMap<u64, (f64, f64)>,
    /// A-t-on au moins un tick de référence pour interpoler ?
    has_prev: bool,
    /// Fraction écoulée vers le prochain tick, dans [0, 1] : le curseur
    /// d'interpolation de la frame courante.
    render_alpha: f64,

    // — Cache du terrain —
    terrain: Vec<u8>,
    terrain_valid: bool,
}

impl App {
    fn new() -> Result<App, JsValue> {
        let canvas = document()
            .get_element_by_id("view")
            .ok_or("canvas #view introuvable")?
            .dyn_into::<HtmlCanvasElement>()?;
        let ctx = canvas
            .get_context("2d")?
            .ok_or("contexte 2d indisponible")?
            .dyn_into::<CanvasRenderingContext2d>()?;
        let pop_chart_ctx = document()
            .get_element_by_id("pop-chart")
            .ok_or("canvas #pop-chart introuvable")?
            .dyn_into::<HtmlCanvasElement>()?
            .get_context("2d")?
            .ok_or("contexte 2d indisponible pour #pop-chart")?
            .dyn_into::<CanvasRenderingContext2d>()?;

        let seed = 42;
        let (sim, home) = build_sim(seed, START_AGENTS, 2);

        let mut app = App {
            sim,
            ctx,
            canvas,
            pop_chart_ctx,
            width: 0,
            height: 0,
            camera: Camera {
                cx: home.0 as f64,
                cy: home.1 as f64,
                scale: 2.0,
            },
            layer: Layer::Biome,
            drag: None,
            press: None,
            playing: true,
            speed: SPEEDS[1],
            tick_acc: 0.0,
            last_ts: None,
            seed,
            placing: false,
            selected: None,
            follow: true,
            color_mode: ColorMode::Activity,
            pop_history: VecDeque::new(),
            last_sampled_day: None,
            prev_pos: HashMap::new(),
            prev_fauna: HashMap::new(),
            has_prev: false,
            render_alpha: 1.0,
            terrain: Vec::new(),
            terrain_valid: false,
        };
        app.resize();
        Ok(app)
    }

    /// Boîte englobante des humains (à défaut du gibier), en tuiles :
    /// `(min_x, min_y, max_x, max_y)`. `None` si le monde est vide.
    fn population_bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let mut b: Option<(f64, f64, f64, f64)> = None;
        for (_, pos) in self.sim.agents.query::<&Position>().iter() {
            grow_bounds(&mut b, pos.x, pos.y);
        }
        if b.is_none() {
            for (_, (_, pos)) in self.sim.fauna.query::<(&Herd, &Position)>().iter() {
                grow_bounds(&mut b, pos.x, pos.y);
            }
        }
        b
    }

    /// Cadre la caméra sur **toute** la population — centre *et* zoom — pour
    /// qu'elle reste visible même dispersée sur des kilomètres (en Phase 2, sans
    /// clans, le groupe diffuse librement). À **hystérésis** : on ne recadre que
    /// si le centre a dérivé ou si le zoom nécessaire a changé notablement,
    /// sinon le terrain se recalculerait à chaque frame. Pas pendant un glisser.
    fn follow_population(&mut self) {
        if !self.follow || self.drag.is_some() {
            return;
        }
        let Some((x0, y0, x1, y1)) = self.population_bounds() else {
            return;
        };
        let (mx, my) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        // Marge autour du groupe (~400 m), puis zoom qui fait tenir la boîte.
        let pad = km_to_tiles(0.4);
        let span_x = (x1 - x0 + 2.0 * pad).max(1.0);
        let span_y = (y1 - y0 + 2.0 * pad).max(1.0);
        let fit = (self.width as f64 / span_x)
            .min(self.height as f64 / span_y)
            .clamp(MIN_SCALE, FOLLOW_MAX_SCALE);

        let drift = ((mx - self.camera.cx).powi(2) + (my - self.camera.cy).powi(2)).sqrt();
        let drift_thresh = self.width.min(self.height) as f64 / self.camera.scale * 0.15;
        let zoom_ratio = fit / self.camera.scale;
        if drift > drift_thresh || !(0.8..1.25).contains(&zoom_ratio) {
            self.camera.cx = mx;
            self.camera.cy = my;
            self.camera.scale = fit;
            self.invalidate_terrain();
        }
    }

    /// Régénère entièrement le monde depuis les champs du panneau Paramètres
    /// (seed, humains initiaux, densité de gibier), recadre la caméra sur le
    /// nouveau foyer et remet l'horloge à zéro.
    fn rebuild(&mut self) {
        let seed = read_u64("cfg-seed", self.seed);
        let agents = read_u64("cfg-humans", START_AGENTS as u64) as usize;
        let herd_grid = read_u64("cfg-herds", 2) as i64;
        let (sim, home) = build_sim(seed, agents, herd_grid);
        self.sim = sim;
        self.seed = seed;
        self.camera.cx = home.0 as f64;
        self.camera.cy = home.1 as f64;
        self.tick_acc = 0.0;
        // Les identifiants du nouveau monde n'ont rien à voir avec l'ancien :
        // on repart sans référence d'interpolation.
        self.prev_pos.clear();
        self.prev_fauna.clear();
        self.has_prev = false;
        self.pop_history.clear();
        self.last_sampled_day = None;
        self.selected = None; // les identifiants de l'ancien monde ne valent plus rien
        self.invalidate_terrain();
    }

    /// Pose une bande d'humains (et son gibier) au pixel écran `(px, py)`.
    fn place_at(&mut self, px: f64, py: f64) {
        let (wx, wy) = self.tile_at(px, py);
        let count = read_u64("cfg-band", 20) as usize;
        cairn_sim::scenario::drop_band(
            &mut self.sim,
            (wx.floor() as i64, wy.floor() as i64),
            count,
            6,
        );
    }

    fn resize(&mut self) {
        let win = window();
        let w = win.inner_width().unwrap().as_f64().unwrap() as usize;
        let h = win.inner_height().unwrap().as_f64().unwrap() as usize;
        self.width = w.max(1);
        self.height = h.max(1);
        self.canvas.set_width(self.width as u32);
        self.canvas.set_height(self.height as u32);
        self.terrain_valid = false;
    }

    fn tile_at(&self, px: f64, py: f64) -> (f64, f64) {
        render::tile_at(
            px, py, self.camera.cx, self.camera.cy, self.camera.scale, self.width, self.height,
        )
    }

    /// Une frame : avance la sim selon le temps écoulé, puis redessine.
    fn frame(&mut self, ts: f64) {
        // dt borné : après un onglet masqué, on ne simule pas des minutes d'un
        // coup. La première frame ne fait qu'amorcer l'horloge.
        let dt_s = match self.last_ts {
            Some(prev) => ((ts - prev) / 1000.0).clamp(0.0, 0.1),
            None => 0.0,
        };
        self.last_ts = Some(ts);

        if self.playing {
            self.tick_acc += dt_s * self.speed;
            let mut stepped = 0;
            while self.tick_acc >= 1.0 && stepped < MAX_TICKS_PER_FRAME {
                // On mémorise les positions **avant** de simuler : ce sont les
                // « prev » depuis lesquels on interpolera jusqu'au nouvel état.
                self.snapshot_positions();
                self.sim.step();
                self.tick_acc -= 1.0;
                stepped += 1;
            }
            if stepped > 0 {
                self.has_prev = true;
                self.sample_population();
            }
        }
        // Curseur d'interpolation : où en est-on entre le dernier tick et le
        // prochain. À l'arrêt (ou sans référence), on colle à l'état courant.
        self.render_alpha = if self.playing && self.has_prev {
            self.tick_acc.clamp(0.0, 1.0)
        } else {
            1.0
        };

        self.follow_population();
        self.render();
    }

    /// Capture la position courante de chaque entité (par identifiant stable)
    /// dans les tables `prev`, pour l'interpolation du prochain intervalle.
    fn snapshot_positions(&mut self) {
        self.prev_pos.clear();
        for (_, (id, pos)) in self.sim.agents.query::<(&AgentId, &Position)>().iter() {
            self.prev_pos.insert(id.0, (pos.x, pos.y));
        }
        self.prev_fauna.clear();
        for (_, (id, pos)) in self.sim.fauna.query::<(&FaunaId, &Position)>().iter() {
            self.prev_fauna.insert(id.0, (pos.x, pos.y));
        }
    }

    /// Empile un point de population dans l'historique, une fois par jour de
    /// jeu (pas par tick — inutile pour une courbe qu'on regarde de loin).
    fn sample_population(&mut self) {
        let day = self.sim.time.tick / cairn_core::TICKS_PER_DAY;
        if self.last_sampled_day == Some(day) {
            return;
        }
        self.last_sampled_day = Some(day);
        self.pop_history.push_back((self.sim.time.tick, self.sim.population() as u32));
        if self.pop_history.len() > POP_HISTORY_CAP {
            self.pop_history.pop_front();
        }
    }

    fn render(&mut self) {
        if !self.terrain_valid {
            self.terrain = render::render_to_buffer(
                self.sim.world.worldgen(),
                self.camera.cx,
                self.camera.cy,
                self.camera.scale,
                self.layer,
                self.width,
                self.height,
            );
            self.terrain_valid = true;
        }
        let img = ImageData::new_with_u8_clamped_array_and_sh(
            Clamped(&self.terrain),
            self.width as u32,
            self.height as u32,
        )
        .expect("ImageData");
        self.ctx.put_image_data(&img, 0.0, 0.0).expect("put_image_data");

        self.draw_entities();
        self.update_readout();
        self.update_population_stats();
        self.update_clan_panel();
        self.update_agent_panel();
        self.draw_population_chart();
    }

    /// Dessine agents et faune par-dessus le terrain. Chaque entité est
    /// **interpolée** entre sa position au tick précédent (`prev_*`) et sa
    /// position courante, selon `render_alpha` — c'est ce qui transforme les
    /// sauts d'une heure de jeu en un glissement fluide. Coordonnées monde →
    /// écran, culling large, petits carrés.
    fn draw_entities(&self) {
        let (w, h) = (self.width as f64, self.height as f64);
        let (cx, cy, scale) = (self.camera.cx, self.camera.cy, self.camera.scale);
        let a = self.render_alpha;
        let visible = |sx: f64, sy: f64| sx > -20.0 && sx < w + 20.0 && sy > -20.0 && sy < h + 20.0;
        // Position interpolée → pixel écran. `prev` par défaut = position
        // courante (entité nouvelle-née : pas de saut, on l'affiche sur place).
        let place = |prev: Option<&(f64, f64)>, curx: f64, cury: f64| {
            let (px, py) = prev.copied().unwrap_or((curx, cury));
            let wx = px + (curx - px) * a;
            let wy = py + (cury - py) * a;
            ((wx - cx) * scale + w / 2.0, (wy - cy) * scale + h / 2.0)
        };

        // — La couche « clan » (Phase 4), dessinée en premier, sous les
        //   entités, comme un fond : territoire diffusé, tensions, foyers,
        //   structures. Une couleur par `ClanId` (angle d'or, cf. `clan_color`).
        let to_screen =
            |wx: f64, wy: f64| ((wx - cx) * scale + w / 2.0, (wy - cy) * scale + h / 2.0);

        // Territoire diffusé (incrément 7) : l'étendue du champ `claim_at`
        // autour de chaque foyer — un disque de rayon `RESIDENCE_RADIUS_TILES`,
        // tracé faiblement pour rester un fond. Là où deux disques se
        // recouvrent, c'est la zone que les clans se disputent.
        let terr_r = cairn_sim::social::RESIDENCE_RADIUS_TILES * scale;
        self.ctx.set_line_width(1.0);
        self.ctx.set_global_alpha(0.28);
        for clan in &self.sim.clans {
            let (sx, sy) = to_screen(clan.home.0, clan.home.1);
            self.ctx.set_stroke_style_str(&clan_color(clan.id));
            self.ctx.begin_path();
            self.ctx.arc(sx, sy, terr_r, 0.0, std::f64::consts::TAU).expect("arc");
            self.ctx.stroke();
        }
        self.ctx.set_global_alpha(1.0);

        // Tensions inter-clans (incrément 6) : un trait rouge entre deux
        // foyers, d'autant plus épais et opaque que la tension est vive.
        self.ctx.set_stroke_style_str("#e0503a");
        for i in 0..self.sim.clans.len() {
            for j in (i + 1)..self.sim.clans.len() {
                let (a, b) = (&self.sim.clans[i], &self.sim.clans[j]);
                let t = self.sim.clan_relations.tension_between(a.id, b.id);
                if t < 0.05 {
                    continue;
                }
                let (ax, ay) = to_screen(a.home.0, a.home.1);
                let (bx, by) = to_screen(b.home.0, b.home.1);
                self.ctx.set_global_alpha(f64::from(t).clamp(0.2, 0.9));
                self.ctx.set_line_width(1.0 + 3.0 * f64::from(t));
                self.ctx.begin_path();
                self.ctx.move_to(ax, ay);
                self.ctx.line_to(bx, by);
                self.ctx.stroke();
            }
        }
        self.ctx.set_global_alpha(1.0);

        // Foyers de clan : un cercle plein au centre du territoire.
        self.ctx.set_line_width(2.0);
        for clan in &self.sim.clans {
            let (sx, sy) = to_screen(clan.home.0, clan.home.1);
            if !visible(sx, sy) {
                continue;
            }
            let r = (scale * 6.0).clamp(6.0, 40.0);
            self.ctx.set_stroke_style_str(&clan_color(clan.id));
            self.ctx.begin_path();
            self.ctx.arc(sx, sy, r, 0.0, std::f64::consts::TAU).expect("arc");
            self.ctx.stroke();
        }

        // Structures bâties (incrément 9) : un petit carré coloré par type au
        // lieu où il a été élevé (hutte brune, grenier or, palissade grise).
        // Elles partagent le foyer du clan : on les décale par type pour
        // qu'un clan qui en possède plusieurs les montre toutes distinctes.
        for st in &self.sim.structures {
            let (bx, by) = to_screen(st.pos.0, st.pos.1);
            let (ox, oy) = structure_offset(st.kind);
            let (sx, sy) = (bx + ox, by + oy);
            if !visible(sx, sy) {
                continue;
            }
            let sz = (scale * 4.0).clamp(5.0, 12.0);
            self.ctx.set_fill_style_str(structure_color(st.kind));
            self.ctx.fill_rect(sx - sz / 2.0, sy - sz / 2.0, sz, sz);
        }

        // Feux de forêt (Phase 5, incrément 5) : un disque orange au foyer du
        // feu, de son rayon de combustion — la 2ᵉ voie d'exposition au feu.
        // Sur fond sombre, un remplissage translucide se lit comme une lueur.
        for fire in &self.sim.fires {
            let (fx, fy) = to_screen(fire.pos.0, fire.pos.1);
            let r = (fire.radius * scale).max(2.0);
            self.ctx.set_global_alpha(0.4);
            self.ctx.set_fill_style_str("#ff6a2a");
            self.ctx.begin_path();
            self.ctx.arc(fx, fy, r, 0.0, std::f64::consts::TAU).expect("arc");
            self.ctx.fill();
        }
        self.ctx.set_global_alpha(1.0);

        // Routes d'expédition (Phase 5, incrément 6) : « le bronze force la
        // route ». Un trait relie l'envoyé à son étape — l'étain lointain à
        // l'aller, le foyer au retour —, avec un repère à l'étape. Rare (il
        // faut la métallurgie du cuivre) mais spectaculaire quand il apparaît.
        if !self.sim.expeditions.is_empty() {
            self.ctx.set_stroke_style_str("#38d6c0");
            self.ctx.set_line_width(1.5);
            for (&aid, exp) in &self.sim.expeditions {
                let envoy = self
                    .sim
                    .agents
                    .query::<(&AgentId, &Position)>()
                    .iter()
                    .find(|(_, (id, _))| id.0 == aid)
                    .map(|(_, (_, pos))| (pos.x, pos.y));
                let Some((ex, ey)) = envoy else { continue };
                let wp = if exp.returning {
                    exp.home
                } else {
                    (exp.tin.0 as f64, exp.tin.1 as f64)
                };
                let (sx, sy) = to_screen(ex, ey);
                let (tx, ty) = to_screen(wp.0, wp.1);
                self.ctx.set_global_alpha(0.75);
                self.ctx.begin_path();
                self.ctx.move_to(sx, sy);
                self.ctx.line_to(tx, ty);
                self.ctx.stroke();
                self.ctx.set_fill_style_str("#38d6c0");
                self.ctx.fill_rect(tx - 3.0, ty - 3.0, 6.0, 6.0);
            }
            self.ctx.set_global_alpha(1.0);
        }

        // Gibier (fauve) : la taille suit l'effectif du troupeau.
        self.ctx.set_fill_style_str("#c8a24a");
        for (_, (id, herd, pos)) in self.sim.fauna.query::<(&FaunaId, &Herd, &Position)>().iter() {
            let (sx, sy) = place(self.prev_fauna.get(&id.0), pos.x, pos.y);
            if !visible(sx, sy) {
                continue;
            }
            let s = ((scale * 1.5) * (1.0 + f64::from(herd.population) / 60.0)).clamp(3.0, 16.0);
            self.ctx.fill_rect(sx - s / 2.0, sy - s / 2.0, s, s);
        }

        // Prédateurs (violet).
        self.ctx.set_fill_style_str("#8b3fb0");
        for (_, (id, pack, pos)) in self.sim.fauna.query::<(&FaunaId, &Pack, &Position)>().iter() {
            let (sx, sy) = place(self.prev_fauna.get(&id.0), pos.x, pos.y);
            if !visible(sx, sy) {
                continue;
            }
            let s = ((scale * 1.5) * (1.0 + f64::from(pack.population) / 8.0)).clamp(3.0, 12.0);
            self.ctx.fill_rect(sx - s / 2.0, sy - s / 2.0, s, s);
        }

        // Humains. Trois modes : couleur fixe, activité (qui chasse, qui
        // boit, qui dort), ou sexe/âge (le portrait démographique). En
        // couleur fixe on pose le style une seule fois. Les enfants sont des
        // carrés plus petits dans tous les modes — on voit la lignée
        // grandir sans ouvrir le moindre panneau.
        let s = scale.clamp(2.5, 8.0);
        let tick = self.sim.time.tick;
        if self.color_mode == ColorMode::Fixed {
            self.ctx.set_fill_style_str(HUMAN_COLOR);
        }
        for (_, (id, pos, behavior, demo)) in self
            .sim
            .agents
            .query::<(&AgentId, &Position, &Behavior, &Demographics)>()
            .iter()
        {
            let (sx, sy) = place(self.prev_pos.get(&id.0), pos.x, pos.y);
            if !visible(sx, sy) {
                continue;
            }
            let adult = demo.is_adult(tick);
            match self.color_mode {
                ColorMode::Fixed => {}
                ColorMode::Activity => self.ctx.set_fill_style_str(activity_color(behavior.activity)),
                ColorMode::SexAge => self.ctx.set_fill_style_str(sex_age_color(demo.sex, adult)),
            }
            let s = if adult { s } else { (s * 0.55).max(2.0) };
            self.ctx.fill_rect(sx - s / 2.0, sy - s / 2.0, s, s);
        }

        // Anneau autour de l'agent inspecté (Phase 5, incrément 8) : on le
        // retrouve par son identifiant et on cercle sa position interpolée.
        if let Some(sel) = self.selected {
            for (_, (id, pos)) in self.sim.agents.query::<(&AgentId, &Position)>().iter() {
                if id.0 != sel {
                    continue;
                }
                let (sx, sy) = place(self.prev_pos.get(&id.0), pos.x, pos.y);
                if visible(sx, sy) {
                    self.ctx.set_stroke_style_str("#ffffff");
                    self.ctx.set_line_width(2.0);
                    self.ctx.begin_path();
                    self.ctx
                        .arc(sx, sy, (s * 1.6).max(7.0), 0.0, std::f64::consts::TAU)
                        .expect("arc");
                    self.ctx.stroke();
                }
                break;
            }
        }
    }

    fn update_readout(&self) {
        if let Some(el) = document().get_element_by_id("readout") {
            let km_per_screen = tiles_to_km(self.width as f64 / self.camera.scale);
            el.set_text_content(Some(&format!(
                "{} · centre ({:.0}, {:.0}) km · largeur {:.0} km",
                self.layer.label(),
                tiles_to_km(self.camera.cx),
                tiles_to_km(self.camera.cy),
                km_per_screen,
            )));
        }
        if let Some(el) = document().get_element_by_id("sim-readout") {
            let (herbivores, predators, _, _) = self.sim.fauna_census();
            let t = self.sim.time;
            let children = self
                .sim
                .agents
                .query::<&Demographics>()
                .iter()
                .filter(|(_, d)| !d.is_adult(t.tick))
                .count();
            el.set_text_content(Some(&format!(
                "An {}, jour {} · {} humains (dont {} enfants, {} naissances) · {:.0} gibier · {:.0} prédateurs",
                t.year(),
                t.day_of_year(),
                self.sim.population(),
                children,
                self.sim.births.len(),
                herbivores,
                predators,
            )));
        }
        self.update_pyramid();
    }

    /// La pyramide des âges du panneau : tranches de 10 ans, barres unicode.
    /// L'observable démographique de la Phase 3 — une population qui persiste
    /// a une base d'enfants et un sommet d'anciens.
    fn update_pyramid(&self) {
        let Some(el) = document().get_element_by_id("age-pyramid") else {
            return;
        };
        let tick = self.sim.time.tick;
        let mut buckets = [0usize; 8]; // 0-9, 10-19, …, 70+
        for (_, demo) in self.sim.agents.query::<&Demographics>().iter() {
            let age = demo.age_years(tick).max(0.0);
            buckets[((age / 10.0) as usize).min(7)] += 1;
        }
        let total: usize = buckets.iter().sum();
        if total == 0 {
            el.set_text_content(Some("population éteinte"));
            return;
        }
        let mut text = String::new();
        for (i, &n) in buckets.iter().enumerate().rev() {
            if n == 0 {
                continue;
            }
            // Barres proportionnelles, 24 colonnes au plus.
            let bar = "█".repeat(1 + n * 23 / total.max(1));
            let label = if i == 7 { "70+".to_string() } else { format!("{:>2}-{}", i * 10, i * 10 + 9) };
            text.push_str(&format!("{label:>5} {bar} {n}\n"));
        }
        el.set_text_content(Some(&text));
    }

    /// Onglet Population : effectifs par sexe/âge, naissances, décès par
    /// cause, savoirs moyens (territoire connu, compétences). Un seul
    /// passage sur la population pour tout calculer.
    fn update_population_stats(&self) {
        let tick = self.sim.time.tick;
        let (mut women, mut men, mut children) = (0u32, 0u32, 0u32);
        let (mut cells_sum, mut springs_sum) = (0usize, 0usize);
        let (mut forage_sum, mut hunt_sum) = (0.0f32, 0.0f32);
        let mut n = 0u32;
        for (_, (demo, mem, sk)) in
            self.sim.agents.query::<(&Demographics, &Memory, &Skills)>().iter()
        {
            match (demo.is_adult(tick), demo.sex) {
                (true, Sex::Female) => women += 1,
                (true, Sex::Male) => men += 1,
                (false, _) => children += 1,
            }
            cells_sum += mem.known.len();
            springs_sum += mem.springs.len();
            forage_sum += sk.foraging;
            hunt_sum += sk.hunting;
            n += 1;
        }

        if let Some(el) = document().get_element_by_id("pop-stats") {
            let (mut starved, mut dehydrated, mut frozen, mut old_age) = (0u32, 0u32, 0u32, 0u32);
            for d in &self.sim.deaths {
                match d.cause {
                    DeathCause::Starvation => starved += 1,
                    DeathCause::Dehydration => dehydrated += 1,
                    DeathCause::Hypothermia => frozen += 1,
                    DeathCause::OldAge => old_age += 1,
                }
            }
            el.set_text_content(Some(&format!(
                "Population   {n:>4}   ({women} ♀ · {men} ♂ · {children} enfants)\n\
                 Naissances   {:>4}\n\
                 Décès        {:>4}   (faim {starved} · soif {dehydrated} · froid {frozen} · vieillesse {old_age})",
                self.sim.births.len(),
                self.sim.deaths.len(),
            )));
        }

        if let Some(el) = document().get_element_by_id("pop-knowledge") {
            if n == 0 {
                el.set_text_content(Some("population éteinte"));
                return;
            }
            let cell_km = tiles_to_km(cairn_sim::memory::MEMORY_CELL_TILES as f64);
            let cells_mean = cells_sum as f64 / f64::from(n);
            el.set_text_content(Some(&format!(
                "Territoire connu   {:.0} km² en moyenne ({cells_mean:.0} cellules)\n\
                 Sources connues    {:.1} par tête\n\
                 Cueillette         {:.2}\n\
                 Chasse             {:.2}",
                cells_mean * cell_km * cell_km,
                springs_sum as f64 / f64::from(n),
                forage_sum / n as f32,
                hunt_sum / n as f32,
            )));
        }
    }

    /// Panneau d'inspection des clans (onglet Population, Phase 4) :
    /// effectif, stock (relatif à son plafond) et âge de chaque clan actif —
    /// jusqu'ici, `sim.clans` ne se lisait que dans les tests. `sim.clans`
    /// est déjà trié par `ClanId` (`social::detect_clans`), donc l'ordre
    /// d'affichage est stable d'une frame à l'autre sans tri ici.
    fn update_clan_panel(&self) {
        let Some(el) = document().get_element_by_id("clan-panel") else {
            return;
        };
        if self.sim.clans.is_empty() {
            el.set_text_content(Some("aucun clan formé"));
            return;
        }
        let tick = self.sim.time.tick;
        let mut lines = Vec::new();
        for clan in &self.sim.clans {
            let age_days = tick.saturating_sub(clan.founded_tick) / cairn_core::TICKS_PER_DAY;
            let cap = cairn_sim::sim::STOCK_CAP_PER_MEMBER * clan.members.len() as f32;
            lines.push(format!(
                "#{:<3} {:>3} membres   stock {:>4.1}/{cap:<4.1}   {age_days:>3} j",
                clan.id.0,
                clan.members.len(),
                clan.stock,
            ));

            // Chef (incrément 8) + tension maximale avec un voisin (incrément 6).
            let max_tension = self
                .sim
                .clans
                .iter()
                .filter(|o| o.id != clan.id)
                .map(|o| self.sim.clan_relations.tension_between(clan.id, o.id))
                .fold(0.0_f32, f32::max);
            let mut second = format!("     chef #{}", clan.chief.0);
            if max_tension >= 0.05 {
                second.push_str(&format!("   tension max {max_tension:.2}"));
            }
            // Ce que le clan cherche à bâtir (incrément 9), s'il désire qqch.
            if let Some(kind) = clan.desired {
                second.push_str(&format!("   veut {}", structure_name(kind)));
            }
            lines.push(second);

            // Âge dérivé + corpus de savoirs (Phase 5) : l'union des techs des
            // membres et l'étiquette d'âge qui en découle (jamais stockée, voir
            // `clan_age`). C'est ici qu'on *voit* l'Histoire d'un clan avancer.
            let corpus: Vec<&str> = self
                .sim
                .clan_corpus(clan.id)
                .iter()
                .map(|&t| self.sim.tech_tree.get(t).label.as_str())
                .collect();
            lines.push(format!(
                "     {} · sait : {}",
                self.sim.clan_age(clan.id).label(),
                if corpus.is_empty() { "rien encore".to_string() } else { corpus.join(", ") },
            ));

            // Structures possédées (incrément 9), triées et comptées par type.
            let mut owned: Vec<&str> = self
                .sim
                .structures
                .iter()
                .filter(|s| s.clan == clan.id)
                .map(|s| structure_name(s.kind))
                .collect();
            owned.sort_unstable();
            if !owned.is_empty() {
                lines.push(format!("     structures : {}", owned.join(", ")));
            }
        }
        el.set_text_content(Some(&lines.join("\n")));
    }

    /// Panneau d'inspection d'un agent (onglet Agent, Phase 5 incrément 8) :
    /// l'individu sélectionné au clic, avec tout ce que la simulation sait de
    /// lui — besoins, traits, compétences, tâche en cours, expositions,
    /// savoir-faire, lignée. On construit toutes les sections en texte pendant
    /// l'unique emprunt de la sim, puis on écrit le DOM ; si l'agent a disparu
    /// (mort, ou évincé du monde résident), on oublie la sélection.
    fn update_agent_panel(&mut self) {
        let others = ["agent-needs", "agent-traits", "agent-action", "agent-why", "agent-lore"];
        let Some(sel) = self.selected else {
            set_text("agent-ident", "Cliquez un humain sur la carte pour l'inspecter.");
            for id in others {
                set_text(id, "");
            }
            return;
        };
        // La pile de motivations : on rejoue la délibération de l'agent (sans
        // effet de bord). `None` = agent disparu ou nourrisson (porté). On la
        // calcule **avant** l'emprunt immuable qui suit (elle veut `&mut sim`).
        let why = self.sim.inspect_agent(AgentId(sel)).map_or_else(
            || "nourrisson — porté, ne délibère pas".to_string(),
            |ms| {
                ms.iter()
                    .map(|m| {
                        format!(
                            "{:<22} {} {:>3.0}%",
                            task_name(m.kind),
                            gauge(m.probability),
                            m.probability * 100.0,
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            },
        );
        let tick = self.sim.time.tick;
        let sections: Option<[String; 5]> = {
            let mut found = None;
            for (
                _,
                (id, phys, demo, traits, skills, beh, exp, know, mem, kin, membership, prestige, carrying),
            ) in self
                .sim
                .agents
                .query::<(
                    &AgentId,
                    &Physiology,
                    &Demographics,
                    &Traits,
                    &Skills,
                    &Behavior,
                    &Exposures,
                    &Knowledge,
                    &Memory,
                    &Kinship,
                    &cairn_sim::ClanMembership,
                    &cairn_sim::agent::Prestige,
                    &cairn_sim::agent::Carrying,
                )>()
                .iter()
            {
                if id.0 != sel {
                    continue;
                }
                let sex = match demo.sex {
                    Sex::Female => "♀",
                    Sex::Male => "♂",
                };
                let stage = if demo.is_infant(tick) {
                    "nourrisson"
                } else if demo.is_adult(tick) {
                    "adulte"
                } else {
                    "enfant"
                };
                let clan = match membership.0 {
                    Some(cid) => format!("clan #{} · {}", cid.0, self.sim.clan_age(cid).label()),
                    None => "sans clan".to_string(),
                };
                let ident = format!(
                    "Humain #{}   {sex}   {:.0} ans ({stage})   {clan}",
                    id.0,
                    demo.age_years(tick).max(0.0),
                );

                let need = |label: &str, v: f32| format!("{label:<8} {} {:>3.0}%", gauge(v), v * 100.0);
                let needs = [
                    need("Santé", phys.health),
                    need("Faim", phys.hunger),
                    need("Soif", phys.thirst),
                    need("Fatigue", phys.fatigue),
                    need("Froid", phys.cold),
                ]
                .join("\n");

                let trait_row = |label: &str, v: f32| format!("{label:<12} {}", gauge(v));
                let traits_s = [
                    trait_row("Force", traits.strength),
                    trait_row("Endurance", traits.endurance),
                    trait_row("Dextérité", traits.dexterity),
                    trait_row("Curiosité", traits.curiosity),
                    trait_row("Sociabilité", traits.sociability),
                    trait_row("Agressivité", traits.aggression),
                    String::new(),
                    trait_row("Cueillette", skills.foraging),
                    trait_row("Chasse", skills.hunting),
                    trait_row("Oratoire", skills.oratory),
                ]
                .join("\n");

                let task = beh.task.map_or_else(|| "—".to_string(), |t| task_name(t.kind));
                let mut action =
                    format!("Tâche      {task}\nActivité   {}\nPrestige   {:.1}", activity_name(beh.activity), prestige.0);
                if carrying.0 > 0.01 {
                    action.push_str(&format!("\nPorte      {:.1} de gibier", carrying.0));
                }

                let seen: Vec<&str> =
                    Exposure::ALL.iter().filter(|&&e| exp.has(e)).map(|&e| exposure_name(e)).collect();
                let techs: Vec<&str> =
                    know.iter().map(|t| self.sim.tech_tree.get(t).label.as_str()).collect();
                let lineage = match (kin.mother, kin.father) {
                    (None, None) => "fondateur (sans ascendance)".to_string(),
                    (m, f) => format!(
                        "mère {} · père {}",
                        m.map_or("?".to_string(), |a| format!("#{}", a.0)),
                        f.map_or("?".to_string(), |a| format!("#{}", a.0)),
                    ),
                };
                let lore = format!(
                    "A vu       {}\nSait faire {}\nLignée     {lineage}\nTerritoire {} cellules · {} sources",
                    if seen.is_empty() { "rien encore".to_string() } else { seen.join(", ") },
                    if techs.is_empty() { "rien encore".to_string() } else { techs.join(", ") },
                    mem.known.len(),
                    mem.springs.len(),
                );

                found = Some([ident, needs, traits_s, action, lore]);
                break;
            }
            found
        };

        match sections {
            None => {
                self.selected = None;
                set_text("agent-ident", "Cet humain n'est plus (mort, ou hors de la scène).");
                for id in others {
                    set_text(id, "");
                }
            }
            Some([ident, needs, traits_s, action, lore]) => {
                set_text("agent-ident", &ident);
                set_text("agent-needs", &needs);
                set_text("agent-traits", &traits_s);
                set_text("agent-action", &action);
                set_text("agent-why", &why);
                set_text("agent-lore", &lore);
            }
        }
    }

    /// Le petit graphique d'évolution de la population (onglet Population) :
    /// une simple ligne reliant les échantillons journaliers.
    fn draw_population_chart(&self) {
        let ctx = &self.pop_chart_ctx;
        ctx.clear_rect(0.0, 0.0, POP_CHART_W, POP_CHART_H);
        if self.pop_history.len() < 2 {
            return;
        }
        let (min_p, max_p) = self.pop_history.iter().fold((u32::MAX, 0u32), |(lo, hi), &(_, p)| {
            (lo.min(p), hi.max(p))
        });
        let span = f64::from(max_p - min_p).max(1.0);
        let n = self.pop_history.len();
        ctx.begin_path();
        ctx.set_stroke_style_str("#6ea8fe");
        ctx.set_line_width(1.5);
        for (i, &(_, p)) in self.pop_history.iter().enumerate() {
            let x = i as f64 / (n - 1) as f64 * POP_CHART_W;
            let y = POP_CHART_H - 3.0 - (f64::from(p - min_p) / span) * (POP_CHART_H - 6.0);
            if i == 0 {
                ctx.move_to(x, y);
            } else {
                ctx.line_to(x, y);
            }
        }
        ctx.stroke();
    }

    fn invalidate_terrain(&mut self) {
        self.terrain_valid = false;
    }

    fn on_pointer_down(&mut self, px: f64, py: f64) {
        self.drag = Some((px, py));
        self.press = Some((px, py));
    }

    fn on_pointer_move(&mut self, px: f64, py: f64) {
        if let Some((lx, ly)) = self.drag {
            // Glisser à la main reprend la main : on cesse de suivre.
            if self.follow && (px != lx || py != ly) {
                self.set_follow(false);
            }
            self.camera.cx -= (px - lx) / self.camera.scale;
            self.camera.cy -= (py - ly) / self.camera.scale;
            self.drag = Some((px, py));
            self.invalidate_terrain();
        }
    }

    fn on_pointer_up(&mut self, px: f64, py: f64) {
        self.drag = None;
        // Un relâchement proche du point d'appui est un **clic** (pas un pan).
        if let Some((dx, dy)) = self.press {
            let moved = (px - dx).hypot(py - dy);
            if moved < 5.0 {
                if self.placing {
                    // Mode placement : le clic pose une bande d'humains.
                    self.place_at(px, py);
                } else {
                    // Sinon, le clic **sélectionne** l'humain le plus proche
                    // pour l'inspecter (ou désélectionne si le clic tombe dans
                    // le vide). On bascule alors sur l'onglet Agent.
                    self.selected = self.pick_agent(px, py);
                    if self.selected.is_some() {
                        activate_tab("agent");
                    }
                }
            }
        }
        self.press = None;
    }

    /// L'humain dont le carré à l'écran est le plus proche du pixel `(px, py)`,
    /// s'il est à moins de `PICK_RADIUS_PX`. Renvoie son `AgentId`. Utilise les
    /// positions **courantes** (l'écart avec la position interpolée affichée est
    /// sous le seuil de sélection). Balayage linéaire : la scène du client est
    /// petite, et c'est un événement de clic, pas une boucle chaude.
    fn pick_agent(&self, px: f64, py: f64) -> Option<u64> {
        let (w, h) = (self.width as f64, self.height as f64);
        let (cx, cy, scale) = (self.camera.cx, self.camera.cy, self.camera.scale);
        let mut best: Option<(u64, f64)> = None;
        for (_, (id, pos)) in self.sim.agents.query::<(&AgentId, &Position)>().iter() {
            let sx = (pos.x - cx) * scale + w / 2.0;
            let sy = (pos.y - cy) * scale + h / 2.0;
            let d = (sx - px).hypot(sy - py);
            if d <= PICK_RADIUS_PX && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((id.0, d));
            }
        }
        best.map(|(id, _)| id)
    }

    fn toggle_color_mode(&mut self) {
        self.color_mode = self.color_mode.next();
        if let Some(el) = document().get_element_by_id("color-toggle") {
            el.set_text_content(Some(self.color_mode.label()));
        }
        // Chaque légende n'a de sens que dans son propre mode.
        set_display("legend-activity", self.color_mode == ColorMode::Activity);
        set_display("legend-sexage", self.color_mode == ColorMode::SexAge);
    }

    fn set_follow(&mut self, on: bool) {
        self.follow = on;
        if let Some(el) = document().get_element_by_id("follow-toggle") {
            let _ = el.class_list().toggle_with_force("active", on);
        }
        if on {
            // Recadre immédiatement (l'hystérésis de `follow_population` ne se
            // déclencherait pas si la caméra est déjà « proche »).
            if let Some((x0, y0, x1, y1)) = self.population_bounds() {
                self.camera.cx = (x0 + x1) / 2.0;
                self.camera.cy = (y0 + y1) / 2.0;
                let pad = km_to_tiles(0.4);
                let fit = (self.width as f64 / (x1 - x0 + 2.0 * pad).max(1.0))
                    .min(self.height as f64 / (y1 - y0 + 2.0 * pad).max(1.0))
                    .clamp(MIN_SCALE, FOLLOW_MAX_SCALE);
                self.camera.scale = fit;
                self.invalidate_terrain();
            }
        }
    }

    fn toggle_placing(&mut self) {
        self.placing = !self.placing;
        if let Some(el) = document().get_element_by_id("place-toggle") {
            el.set_text_content(Some(if self.placing {
                "✓ Cliquez sur la carte"
            } else {
                "Poser des humains"
            }));
            let _ = el.class_list().toggle_with_force("armed", self.placing);
        }
        // Le curseur signale le mode.
        let _ = self.canvas.style().set_property(
            "cursor",
            if self.placing { "crosshair" } else { "grab" },
        );
    }

    fn on_wheel(&mut self, delta_y: f64, mx: f64, my: f64) {
        let (wx, wy) = self.tile_at(mx, my);
        let factor = (-delta_y * 0.0015).exp();
        self.camera.scale = (self.camera.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        self.camera.cx = wx - (mx - self.width as f64 / 2.0) / self.camera.scale;
        self.camera.cy = wy - (my - self.height as f64 / 2.0) / self.camera.scale;
        self.invalidate_terrain();
    }

    fn on_resize(&mut self) {
        self.resize();
    }

    fn set_layer(&mut self, layer: Layer) {
        self.layer = layer;
        self.invalidate_terrain();
    }

    fn toggle_play(&mut self) {
        self.playing = !self.playing;
        if let Some(el) = document().get_element_by_id("play-toggle") {
            el.set_text_content(Some(if self.playing { "⏸ Pause" } else { "▶ Lecture" }));
        }
    }

    fn set_speed(&mut self, speed: f64) {
        self.speed = speed;
        if !self.playing {
            self.toggle_play();
        }
    }
}

/// Construit un `Sim` (humidité rapide, comme en Phase 1) et y sème une
/// population avec son gibier près d'un continent connu de la seed. Renvoie la
/// sim et le foyer retenu.
fn build_sim(seed: u64, agents: usize, herd_grid: i64) -> (Sim, (i64, i64)) {
    let cfg = WorldGenConfig {
        humidity: HumidityConfig {
            steps: 32,
            lateral_samples: 0,
            ..HumidityConfig::default()
        },
        ..WorldGenConfig::default()
    };
    let mut sim = Sim::with_config(WorldSeed(seed), CHUNK_CAPACITY, cfg);
    let seed_point = (km_to_tiles(1500.0) as i64, km_to_tiles(2100.0) as i64);
    let home = cairn_sim::scenario::find_home(&mut sim, seed_point);
    cairn_sim::scenario::populate(&mut sim, home, agents, herd_grid);
    (sim, home)
}

/// Étend une boîte englobante `(min_x, min_y, max_x, max_y)` pour inclure
/// le point `(x, y)`.
fn grow_bounds(b: &mut Option<(f64, f64, f64, f64)>, x: f64, y: f64) {
    *b = Some(match *b {
        None => (x, y, x, y),
        Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
    });
}

/// Lit un paramètre `?clé=nombre` de l'URL, s'il est présent et numérique.
fn query_param_u64(key: &str) -> Option<u64> {
    let search = window().location().search().ok()?;
    let needle = format!("{key}=");
    search
        .trim_start_matches('?')
        .split('&')
        .find_map(|kv| kv.strip_prefix(&needle))
        .and_then(|v| v.parse::<u64>().ok())
}

/// Lit un champ `<input>` numérique par id ; renvoie `fallback` s'il est
/// absent ou illisible.
fn read_u64(id: &str, fallback: u64) -> u64 {
    document()
        .get_element_by_id(id)
        .and_then(|el| el.dyn_into::<web_sys::HtmlInputElement>().ok())
        .and_then(|input| input.value().trim().parse::<u64>().ok())
        .unwrap_or(fallback)
}

/// Couleur unique des humains en mode « couleur fixe ».
const HUMAN_COLOR: &str = "#e8503a";

/// Couleur d'un agent selon ce qu'il fait — la lisibilité du « pourquoi »
/// chère au brief, en un coup d'œil.
fn activity_color(activity: Activity) -> &'static str {
    match activity {
        Activity::Idle | Activity::Walking => "#e8503a", // rouge : en chemin
        Activity::Eating | Activity::Hunting => "#ff9a3c", // orange : se nourrit
        Activity::Drinking => "#46b4ff",                 // bleu : boit
        Activity::Sleeping => "#6a6ad0",                 // indigo : dort
        Activity::Sheltering => "#b070c8",               // mauve : s'abrite
    }
}

/// Couleur d'un agent selon son sexe et son stade de vie — le portrait
/// démographique de la population en un coup d'œil.
fn sex_age_color(sex: Sex, adult: bool) -> &'static str {
    if !adult {
        return "#f2c94c"; // jaune : enfant, indépendamment du sexe
    }
    match sex {
        Sex::Female => "#e85ca0", // rose
        Sex::Male => "#4a90e2",   // bleu
    }
}

/// Couleur déterministe du halo de foyer d'un clan. Le nombre de clans
/// n'est pas borné (contrairement au sexe/l'activité, des enums fermées) :
/// angle d'or (~137°) plutôt qu'une palette figée, pour que deux clans
/// voisins en id restent visuellement distincts même si beaucoup coexistent.
fn clan_color(id: cairn_sim::ClanId) -> String {
    let hue = (id.0.wrapping_mul(137) % 360) as f64;
    format!("hsl({hue}, 70%, 55%)")
}

/// Couleur du marqueur d'une structure sur la carte, par type.
fn structure_color(kind: StructureKind) -> &'static str {
    match kind {
        StructureKind::Hut => "#a9733e",      // hutte : brun terre
        StructureKind::Granary => "#d8b24a",  // grenier : or/paille
        StructureKind::Palisade => "#9aa0a6", // palissade : bois grisé
        StructureKind::ChiefHut => "#c25b3a", // hutte du chef : terre cuite, le siège
    }
}

/// Petit décalage écran (px) par type, pour ne pas empiler au foyer les
/// structures d'un même clan — hutte/grenier/palissade/hutte du chef.
fn structure_offset(kind: StructureKind) -> (f64, f64) {
    match kind {
        StructureKind::Hut => (-8.0, -6.0),
        StructureKind::Granary => (8.0, -6.0),
        StructureKind::Palisade => (0.0, 9.0),
        StructureKind::ChiefHut => (0.0, -10.0), // au sommet : le siège du clan
    }
}

/// Nom lisible d'une structure, pour le panneau texte.
fn structure_name(kind: StructureKind) -> &'static str {
    match kind {
        StructureKind::Hut => "hutte",
        StructureKind::Granary => "grenier",
        StructureKind::Palisade => "palissade",
        StructureKind::ChiefHut => "hutte du chef",
    }
}

/// Écrit le texte d'un élément par id, s'il existe (aucun effet sinon) —
/// raccourci pour remplir les blocs du panneau d'agent depuis Rust.
fn set_text(id: &str, text: &str) {
    if let Some(el) = document().get_element_by_id(id) {
        el.set_text_content(Some(text));
    }
}

/// Une jauge unicode de 10 cases pour une valeur dans [0, 1] : le vocabulaire
/// visuel déjà employé par la pyramide des âges (`█`), sans nouvelle CSS.
fn gauge(v: f32) -> String {
    let filled = (v.clamp(0.0, 1.0) * 10.0).round() as usize;
    let mut s = String::with_capacity(10 * 3);
    for i in 0..10 {
        s.push(if i < filled { '█' } else { '░' });
    }
    s
}

/// Nom lisible de la tâche en cours, pour le panneau d'agent — le « pourquoi »
/// du brief rendu en clair.
fn task_name(kind: TaskKind) -> String {
    match kind {
        TaskKind::Drink => "boire".to_string(),
        TaskKind::Forage => "cueillir".to_string(),
        TaskKind::Hunt => "chasser".to_string(),
        TaskKind::Sleep => "dormir".to_string(),
        TaskKind::Shelter => "s'abriter".to_string(),
        TaskKind::Wander => "errer".to_string(),
        TaskKind::Follow => "suivre un parent".to_string(),
        TaskKind::Socialize => "rejoindre les siens".to_string(),
        TaskKind::Explore => "explorer l'inconnu".to_string(),
        TaskKind::ReturnToClan => "rentrer au clan".to_string(),
        TaskKind::EatFromStock => "puiser dans le stock".to_string(),
        TaskKind::BringSurplusHome => "rapporter du gibier".to_string(),
        TaskKind::Build(k) => format!("bâtir : {}", structure_name(k)),
        TaskKind::Expedition => "expédition (chercher l'étain)".to_string(),
    }
}

/// Nom lisible de l'activité de l'heure (le pendant textuel d'`activity_color`).
fn activity_name(a: Activity) -> &'static str {
    match a {
        Activity::Idle => "au repos",
        Activity::Walking => "en chemin",
        Activity::Eating => "se nourrit",
        Activity::Drinking => "boit",
        Activity::Sleeping => "dort",
        Activity::Sheltering => "s'abrite",
        Activity::Hunting => "chasse",
    }
}

/// Nom lisible d'une exposition (ce qu'un agent a déjà croisé).
fn exposure_name(e: Exposure) -> &'static str {
    match e {
        Exposure::Flint => "silex",
        Exposure::Clay => "argile",
        Exposure::Obsidian => "obsidienne",
        Exposure::Copper => "cuivre",
        Exposure::Tin => "étain",
        Exposure::Gold => "or",
        Exposure::Iron => "fer",
        Exposure::Wood => "bois",
        Exposure::WildGrasses => "graminées",
        Exposure::Fire => "feu",
    }
}

/// Amène un onglet au premier plan depuis Rust, en répliquant la bascule que
/// fait le JS d'`index.html` (classe `active` sur le bouton, `hidden` sur les
/// panes) — pour que sélectionner un agent ouvre aussitôt l'onglet Agent.
fn activate_tab(name: &str) {
    let doc = document();
    if let Ok(tabs) = doc.query_selector_all(".tab-btn") {
        for i in 0..tabs.length() {
            if let Some(el) = tabs.item(i).and_then(|n| n.dyn_into::<web_sys::Element>().ok()) {
                let on = el.get_attribute("data-tab").as_deref() == Some(name);
                let _ = el.class_list().toggle_with_force("active", on);
            }
        }
    }
    if let Ok(panes) = doc.query_selector_all(".tab-pane") {
        for i in 0..panes.length() {
            if let Some(el) = panes.item(i).and_then(|n| n.dyn_into::<web_sys::Element>().ok()) {
                let hide = el.get_attribute("data-pane").as_deref() != Some(name);
                let _ = el.class_list().toggle_with_force("hidden", hide);
            }
        }
    }
}

/// Affiche ou masque un élément par id (`display: grid`/`none`) — sert aux
/// légendes qui n'ont de sens que dans leur propre mode de couleur.
fn set_display(id: &str, visible: bool) {
    if let Some(el) = document().get_element_by_id(id) {
        let _ = el
            .dyn_ref::<web_sys::HtmlElement>()
            .map(|h| h.style().set_property("display", if visible { "grid" } else { "none" }));
    }
}

// ─────────────────────── boucle d'animation permanente ───────────────────────

/// Programme la prochaine frame, qui se reprogrammera elle-même — une boucle
/// `requestAnimationFrame` continue. `once_into_js` libère chaque closure
/// après appel (pas de fuite par frame) ; le timestamp fourni par rAF sert
/// d'horloge de jeu.
fn schedule_frame() {
    let cb = Closure::once_into_js(move |ts: f64| {
        with_app(|a| a.frame(ts));
        schedule_frame();
    });
    window()
        .request_animation_frame(cb.unchecked_ref())
        .expect("request_animation_frame");
}

// ─────────────────────────── câblage des événements ───────────────────────────

macro_rules! listen {
    ($target:expr, $event:literal, $ty:ty, $body:expr) => {{
        let closure = Closure::<dyn FnMut($ty)>::new($body);
        $target.add_event_listener_with_callback($event, closure.as_ref().unchecked_ref())?;
        closure.forget();
    }};
}

fn install_event_handlers() -> Result<(), JsValue> {
    let canvas = document()
        .get_element_by_id("view")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()?;
    let win = window();

    listen!(canvas, "mousedown", web_sys::MouseEvent, move |e: web_sys::MouseEvent| {
        with_app(|a| a.on_pointer_down(e.client_x() as f64, e.client_y() as f64));
    });
    listen!(win, "mousemove", web_sys::MouseEvent, move |e: web_sys::MouseEvent| {
        with_app(|a| a.on_pointer_move(e.client_x() as f64, e.client_y() as f64));
    });
    listen!(win, "mouseup", web_sys::MouseEvent, move |e: web_sys::MouseEvent| {
        with_app(|a| a.on_pointer_up(e.client_x() as f64, e.client_y() as f64));
    });
    listen!(canvas, "wheel", web_sys::WheelEvent, move |e: web_sys::WheelEvent| {
        e.prevent_default();
        with_app(|a| a.on_wheel(e.delta_y(), e.client_x() as f64, e.client_y() as f64));
    });
    listen!(win, "resize", web_sys::Event, move |_e: web_sys::Event| {
        with_app(App::on_resize);
    });

    // Boutons de couche.
    let buttons = document().query_selector_all(".layer-btn")?;
    for i in 0..buttons.length() {
        let btn = buttons.item(i).unwrap().dyn_into::<web_sys::HtmlElement>()?;
        let id = btn.get_attribute("data-layer").unwrap_or_default();
        listen!(btn, "click", web_sys::MouseEvent, move |_e: web_sys::MouseEvent| {
            if let Some(layer) = Layer::from_id(&id) {
                set_active_button(".layer-btn", "data-layer", &id);
                with_app(|a| a.set_layer(layer));
            }
        });
    }
    set_active_button(".layer-btn", "data-layer", Layer::Biome.id());

    // Pause / lecture.
    if let Some(btn) = document().get_element_by_id("play-toggle") {
        listen!(btn, "click", web_sys::MouseEvent, move |_e: web_sys::MouseEvent| {
            with_app(App::toggle_play);
        });
    }
    // Suivre la population.
    if let Some(btn) = document().get_element_by_id("follow-toggle") {
        listen!(btn, "click", web_sys::MouseEvent, move |_e: web_sys::MouseEvent| {
            with_app(|a| a.set_follow(!a.follow));
        });
    }
    // Mode de couleur des humains (activité ↔ couleur fixe).
    if let Some(btn) = document().get_element_by_id("color-toggle") {
        listen!(btn, "click", web_sys::MouseEvent, move |_e: web_sys::MouseEvent| {
            with_app(App::toggle_color_mode);
        });
    }

    // Boutons de vitesse.
    let speeds = document().query_selector_all(".speed-btn")?;
    for i in 0..speeds.length() {
        let btn = speeds.item(i).unwrap().dyn_into::<web_sys::HtmlElement>()?;
        let val = btn.get_attribute("data-speed").unwrap_or_default();
        listen!(btn, "click", web_sys::MouseEvent, move |_e: web_sys::MouseEvent| {
            if let Ok(speed) = val.parse::<f64>() {
                set_active_button(".speed-btn", "data-speed", &val);
                with_app(|a| a.set_speed(speed));
            }
        });
    }
    set_active_button(".speed-btn", "data-speed", &format!("{}", SPEEDS[1]));

    // Paramètres : régénérer le monde.
    if let Some(btn) = document().get_element_by_id("cfg-apply") {
        listen!(btn, "click", web_sys::MouseEvent, move |_e: web_sys::MouseEvent| {
            with_app(App::rebuild);
        });
    }
    // Paramètres : armer le placement d'humains au clic.
    if let Some(btn) = document().get_element_by_id("place-toggle") {
        listen!(btn, "click", web_sys::MouseEvent, move |_e: web_sys::MouseEvent| {
            with_app(App::toggle_placing);
        });
    }

    Ok(())
}

/// Applique la classe `active` au bouton dont `attr` vaut `value`, dans le
/// groupe `selector`.
fn set_active_button(selector: &str, attr: &str, value: &str) {
    if let Ok(buttons) = document().query_selector_all(selector) {
        for i in 0..buttons.length() {
            if let Some(el) = buttons.item(i).and_then(|n| n.dyn_into::<web_sys::Element>().ok()) {
                let is_active = el.get_attribute(attr).as_deref() == Some(value);
                let _ = el.class_list().toggle_with_force("active", is_active);
            }
        }
    }
}

fn with_app(f: impl FnOnce(&mut App)) {
    APP.with(|slot| {
        if let Some(app) = slot.borrow_mut().as_mut() {
            f(app);
        }
    });
}

fn window() -> web_sys::Window {
    web_sys::window().expect("pas de window")
}

fn document() -> web_sys::Document {
    window().document().expect("pas de document")
}
