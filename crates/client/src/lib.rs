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
    Kinship, Knowledge, Memory, Pack, Physiology, Position, Sex, Sim, Skills, Species,
    StructureKind, TaskKind, Traits,
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
const POP_CHART_W: f64 = 298.0;
const POP_CHART_H: f64 = 52.0;

/// Entrées de Chronique affichées dans le panneau. Le journal complet vit dans
/// la sim (`Sim::chronicle`) et grandit tout au long de la partie ; on n'en rend
/// que la fin — le reste s'atteindra par une vue filtrable (par clan, par
/// individu, par période) quand le serveur la servira.
const CHRONICLE_LINES: usize = 40;

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
            reveal_agent_panel();
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
    /// Horodatage (ms, horloge rAF depuis le chargement) du **début** de la
    /// simulation courante — première frame, ou dernière régénération. Sert à
    /// afficher « en cours depuis » en temps réel. `None` tant qu'aucune frame.
    start_ts: Option<f64>,
    /// Temps réel écoulé depuis `start_ts`, en ms — recalculé chaque frame.
    elapsed_ms: f64,

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
            start_ts: None,
            elapsed_ms: 0.0,
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
        self.start_ts = None; // « en cours depuis » repart de la régénération
        self.elapsed_ms = 0.0;
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
        // Temps réel écoulé depuis le début de cette simulation (« en cours
        // depuis »). La première frame fixe l'origine.
        let start = *self.start_ts.get_or_insert(ts);
        self.elapsed_ms = ts - start;

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
        self.update_chronicle_panel();
        self.update_agent_panel();
        self.draw_population_chart();
    }

    /// La Chronique (Phase 6, BRIEF §6.4) : les derniers faits notables du
    /// monde, du plus récent au plus ancien. Ce panneau est *le* produit de la
    /// phase — c'est lui qu'on lit en revenant après une absence.
    ///
    /// La rédaction vit dans `sim::chronicle` (pure) ; ici on ne fait que la
    /// mettre en forme. Les faits du dernier mois de jeu sont marqués « frais »
    /// pour que l'œil accroche ce qui vient d'arriver.
    fn update_chronicle_panel(&self) {
        if self.sim.chronicle.is_empty() {
            set_html(
                "chronicle-panel",
                "<div class=\"empty\">Rien à raconter encore. Les clans, les découvertes, \
                 les affrontements et les incendies s'écriront ici.</div>",
            );
            return;
        }
        let seed = self.sim.world.seed();
        let recent = self.sim.time.tick.saturating_sub(30 * cairn_core::TICKS_PER_DAY);
        let mut html = String::new();
        for event in self.sim.chronicle.iter().rev().take(CHRONICLE_LINES) {
            let (when, what) =
                cairn_sim::chronicle::tell_parts(event, seed, &self.sim.tech_tree, &self.sim.climate);
            let cls = if event.tick >= recent { "entry fresh" } else { "entry" };
            html.push_str(&format!(
                "<div class=\"{cls}\"><div class=\"e-when\">{when}</div>\
                 <div class=\"e-text\">{what}</div></div>"
            ));
        }
        set_html("chronicle-panel", &html);
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

        // Gibier : la couleur dit l'espèce (cerf, aurochs, renne…), la taille
        // suit l'effectif du troupeau.
        for (_, (id, herd, pos)) in self.sim.fauna.query::<(&FaunaId, &Herd, &Position)>().iter() {
            let (sx, sy) = place(self.prev_fauna.get(&id.0), pos.x, pos.y);
            if !visible(sx, sy) {
                continue;
            }
            let s = ((scale * 1.5) * (1.0 + f64::from(herd.population) / 60.0)).clamp(3.0, 16.0);
            self.ctx.set_fill_style_str(species_color(herd.species));
            self.ctx.fill_rect(sx - s / 2.0, sy - s / 2.0, s, s);
        }

        // Prédateurs : couleur par espèce (loup, lion des cavernes).
        for (_, (id, pack, pos)) in self.sim.fauna.query::<(&FaunaId, &Pack, &Position)>().iter() {
            let (sx, sy) = place(self.prev_fauna.get(&id.0), pos.x, pos.y);
            if !visible(sx, sy) {
                continue;
            }
            let s = ((scale * 1.5) * (1.0 + f64::from(pack.population) / 8.0)).clamp(3.0, 12.0);
            self.ctx.set_fill_style_str(species_color(pack.species));
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
        let t = self.sim.time;
        // Le dock de temps : la date de jeu compacte.
        set_html("dock-date", &format!("An <b>{}</b> · jour <b>{}</b>", t.year(), t.day_of_year()));
        // Readout de cadrage (panneau Carte).
        set_text(
            "readout",
            &format!(
                "{} · centre ({:.0}, {:.0}) km · largeur {:.0} km",
                self.layer.label(),
                tiles_to_km(self.camera.cx),
                tiles_to_km(self.camera.cy),
                tiles_to_km(self.width as f64 / self.camera.scale),
            ),
        );
        // La faune, sous les effectifs.
        let (herbivores, predators, _, _) = self.sim.fauna_census();
        set_text("sim-readout", &format!("{herbivores:.0} gibier · {predators:.0} prédateurs"));
        // « En cours depuis » (temps réel) dans les Paramètres.
        set_text("sim-elapsed", &format_elapsed(self.elapsed_ms));
        self.update_pyramid();
    }

    /// La pyramide des âges du panneau : tranches de 10 ans, barres unicode.
    /// L'observable démographique de la Phase 3 — une population qui persiste
    /// a une base d'enfants et un sommet d'anciens.
    fn update_pyramid(&self) {
        let tick = self.sim.time.tick;
        let mut buckets = [0usize; 8]; // 0-9, 10-19, …, 70+
        for (_, demo) in self.sim.agents.query::<&Demographics>().iter() {
            let age = demo.age_years(tick).max(0.0);
            buckets[((age / 10.0) as usize).min(7)] += 1;
        }
        let total: usize = buckets.iter().sum();
        if total == 0 {
            set_html("age-pyramid", "<div class=\"empty\">population éteinte</div>");
            return;
        }
        // Barres à l'échelle de la tranche la plus peuplée (la forme se lit
        // mieux qu'à l'échelle du total).
        let max = buckets.iter().copied().max().unwrap_or(1).max(1);
        let mut html = String::new();
        for (i, &n) in buckets.iter().enumerate().rev() {
            let label = if i == 7 { "70+".to_string() } else { format!("{}-{}", i * 10, i * 10 + 9) };
            let w = 100.0 * n as f64 / max as f64;
            html.push_str(&format!(
                "<div class=\"pyr\"><span class=\"p-l\">{label}</span><span class=\"bar\"><i style=\"width:{w:.0}%\"></i></span><span class=\"p-v\">{n}</span></div>"
            ));
        }
        set_html("age-pyramid", &html);
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

        // Effectifs (grand total + puces par sexe/âge) et décès par cause.
        let (mut starved, mut dehydrated, mut frozen, mut old_age, mut predated, mut killed, mut struck, mut sick) =
            (0u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
        for d in &self.sim.deaths {
            match d.cause {
                DeathCause::Starvation => starved += 1,
                DeathCause::Dehydration => dehydrated += 1,
                DeathCause::Hypothermia => frozen += 1,
                DeathCause::OldAge => old_age += 1,
                DeathCause::Predation => predated += 1,
                DeathCause::Violence => killed += 1,
                DeathCause::Lightning => struck += 1,
                DeathCause::Disease => sick += 1,
            }
        }
        set_html(
            "pop-stats",
            &format!(
                "<div class=\"tallies\">\
                   <span class=\"tally-big\">{n}</span>\
                   <span class=\"chip\"><span class=\"cd\" style=\"background:#e85ca0\"></span>{women} ♀</span>\
                   <span class=\"chip\"><span class=\"cd\" style=\"background:#4a90e2\"></span>{men} ♂</span>\
                   <span class=\"chip\"><span class=\"cd\" style=\"background:#f2c94c\"></span>{children} enfants</span>\
                 </div>\
                 <div class=\"deaths\">\
                   <span><b>{}</b> naissances</span>\
                   <span><b>{}</b> décès</span>\
                   <span>faim {starved} · soif {dehydrated} · froid {frozen} · vieillesse {old_age} · prédation {predated} · violence {killed} · foudre {struck} · mal {sick}</span>\
                 </div>",
                self.sim.births.len(),
                self.sim.deaths.len(),
            ),
        );

        // Savoirs moyens : compétences en jauges, territoire/sources en lignes.
        if n == 0 {
            set_html("pop-knowledge", "<div class=\"empty\">population éteinte</div>");
            return;
        }
        let cell_km = tiles_to_km(cairn_sim::memory::MEMORY_CELL_TILES as f64);
        let cells_mean = cells_sum as f64 / f64::from(n);
        let forage = forage_sum / n as f32;
        let hunt = hunt_sum / n as f32;
        set_html(
            "pop-knowledge",
            &format!(
                "{}{}\
                 <div class=\"kv\"><span class=\"k\">Territoire connu</span><span class=\"v\">{:.0} km²</span></div>\
                 <div class=\"kv\"><span class=\"k\">Sources / tête</span><span class=\"v\">{:.1}</span></div>",
                gauge_html("Cueillette", forage, &format!("{forage:.2}"), "fill-accent"),
                gauge_html("Chasse", hunt, &format!("{hunt:.2}"), "fill-accent"),
                cells_mean * cell_km * cell_km,
                springs_sum as f64 / f64::from(n),
            ),
        );
    }

    /// Panneau d'inspection des clans (onglet Population, Phase 4) :
    /// effectif, stock (relatif à son plafond) et âge de chaque clan actif —
    /// jusqu'ici, `sim.clans` ne se lisait que dans les tests. `sim.clans`
    /// est déjà trié par `ClanId` (`social::detect_clans`), donc l'ordre
    /// d'affichage est stable d'une frame à l'autre sans tri ici.
    fn update_clan_panel(&self) {
        if self.sim.clans.is_empty() {
            set_html("clan-panel", "<div class=\"empty\">aucun clan formé</div>");
            return;
        }
        let mut html = String::new();
        for clan in &self.sim.clans {
            let n = clan.members.len();
            let cap = cairn_sim::sim::STOCK_CAP_PER_MEMBER * n as f32;
            let frac = if cap > 0.0 { clan.stock / cap } else { 0.0 };
            let age = self.sim.clan_age(clan.id);
            let age_class = if matches!(age, cairn_sim::Age::Paleolithic) {
                "chip age paleo"
            } else {
                "chip age"
            };

            // Tension maximale avec un voisin (incrément 6) + son identité.
            let (mut best_t, mut best_other) = (0.0f32, 0u64);
            for o in &self.sim.clans {
                if o.id == clan.id {
                    continue;
                }
                let t = self.sim.clan_relations.tension_between(clan.id, o.id);
                if t > best_t {
                    best_t = t;
                    best_other = o.id.0;
                }
            }

            // Corpus de savoirs (Phase 5) : l'union des techs des membres.
            let corpus: Vec<&str> = self
                .sim
                .clan_corpus(clan.id)
                .iter()
                .map(|&t| self.sim.tech_tree.get(t).label.as_str())
                .collect();

            // Ligne « chef · sait · tension · veut ». Les protagonistes sont
            // désormais nommés (Phase 6) : un chef a un nom, un rival aussi —
            // c'est ce qui rend le panneau lisible comme une histoire.
            let chief = self
                .sim
                .agent_name(clan.chief)
                .unwrap_or_else(|| format!("#{}", clan.chief.0));
            let mut line = format!("chef <b>{chief}</b>");
            if !corpus.is_empty() {
                line.push_str(&format!(" · sait : <b>{}</b>", corpus.join(", ")));
            }
            if best_t >= 0.05 {
                line.push_str(&format!(
                    " · <span class=\"warn\">tension {best_t:.2} avec les {}</span>",
                    self.sim.clan_name(cairn_sim::ClanId(best_other))
                ));
            }
            if let Some(kind) = clan.desired {
                line.push_str(&format!(" · veut <b>{}</b>", structure_name(kind)));
            }

            // Structures possédées (incrément 9), triées par type.
            let mut owned: Vec<&str> = self
                .sim
                .structures
                .iter()
                .filter(|s| s.clan == clan.id)
                .map(|s| structure_name(s.kind))
                .collect();
            owned.sort_unstable();
            let structs_line = if owned.is_empty() {
                String::new()
            } else {
                format!("<div class=\"c-line\">structures : <b>{}</b></div>", owned.join(", "))
            };

            html.push_str(&format!(
                "<div class=\"clan\">\
                   <div class=\"c-top\">\
                     <span class=\"c-id\" style=\"background:{color}\"></span>\
                     <span class=\"c-name\">Les {name}</span>\
                     <span class=\"{age_class}\">{age_label}</span>\
                     <span class=\"c-meta\">{n} membres</span>\
                   </div>\
                   <div class=\"stockline\"><span class=\"s-l\">stock</span>{bar}<span class=\"s-v\">{stock:.0}/{cap:.0}</span></div>\
                   <div class=\"c-line\">{line}</div>{structs_line}\
                 </div>",
                color = clan_color(clan.id),
                name = self.sim.clan_name(clan.id),
                age_label = age.label(),
                bar = bar_html(frac, health_fill(frac)),
                stock = clan.stock,
            ));
        }
        set_html("clan-panel", &html);
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
            set_html("agent-ident", "<div class=\"empty\">Cliquez un humain sur la carte pour l'inspecter.</div>");
            for id in others {
                set_html(id, "");
            }
            return;
        };
        // La pile de motivations : on rejoue la délibération de l'agent (sans
        // effet de bord), en lignes `motiv` (la plus probable surlignée). `None`
        // = agent disparu ou nourrisson (porté). Calculée **avant** l'emprunt
        // immuable qui suit (elle veut `&mut sim`).
        let why = self.sim.inspect_agent(AgentId(sel)).map(|ms| {
            ms.iter()
                .enumerate()
                .map(|(i, m)| {
                    let cls = if i == 0 { "motiv top" } else { "motiv" };
                    format!(
                        "<div class=\"{cls}\"><span class=\"m-l\">{}</span>{}<span class=\"m-v\">{:.0}%</span></div>",
                        task_name(m.kind),
                        bar_html(m.probability, ""),
                        m.probability * 100.0,
                    )
                })
                .collect::<String>()
        });
        let tick = self.sim.time.tick;
        let sections: Option<[String; 5]> = {
            let mut found = None;
            for (
                _,
                (id, phys, demo, traits, skills, beh, exp, know, mem, kin, membership, prestige, carrying, wound),
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
                    &cairn_sim::Wound,
                )>()
                .iter()
            {
                if id.0 != sel {
                    continue;
                }
                // — Identité : nom + puces (sexe/âge, stade, clan+âge tech) —
                let (sex_sym, sex_col) = match demo.sex {
                    Sex::Female => ("♀", "#e85ca0"),
                    Sex::Male => ("♂", "#4a90e2"),
                };
                let stage = if demo.is_infant(tick) {
                    "nourrisson"
                } else if demo.is_adult(tick) {
                    "adulte"
                } else {
                    "enfant"
                };
                let clan_chip = match membership.0 {
                    Some(cid) => {
                        let age = self.sim.clan_age(cid);
                        let cls = if matches!(age, cairn_sim::Age::Paleolithic) {
                            "chip age paleo"
                        } else {
                            "chip age"
                        };
                        format!(
                            "<span class=\"{cls}\">les {} · {}</span>",
                            self.sim.clan_name(cid),
                            age.label()
                        )
                    }
                    None => "<span class=\"chip\">sans clan</span>".to_string(),
                };
                // Le nom propre remplace le numéro (Phase 6) : c'est la même
                // personne que celle que la Chronique nommera à sa mort.
                let ident = format!(
                    "<div class=\"name\">{}</div>\
                     <div class=\"chips\">\
                       <span class=\"chip\"><span class=\"cd\" style=\"background:{sex_col}\"></span>{sex_sym} {:.0} ans</span>\
                       <span class=\"chip\">{stage}</span>{clan_chip}\
                     </div>",
                    cairn_sim::names::agent_name(self.sim.world.seed(), *id, demo.sex),
                    demo.age_years(tick).max(0.0),
                );

                // — Besoins : jauges couleur d'état (santé haute = vert ; un
                //   besoin haut = alarme) —
                let pc = |v: f32| format!("{:.0}%", v * 100.0);
                let needs = format!(
                    "{}{}{}{}{}{}",
                    gauge_html("Santé", phys.health, &pc(phys.health), health_fill(phys.health)),
                    gauge_html("Faim", phys.hunger, &pc(phys.hunger), need_fill(phys.hunger)),
                    gauge_html("Soif", phys.thirst, &pc(phys.thirst), need_fill(phys.thirst)),
                    gauge_html("Fatigue", phys.fatigue, &pc(phys.fatigue), need_fill(phys.fatigue)),
                    gauge_html("Froid", phys.cold, &pc(phys.cold), need_fill(phys.cold)),
                    // La plaie ne s'affiche que si l'agent est blessé (§3.1).
                    if wound.0 > 0.01 {
                        gauge_html("Plaie", wound.0, &pc(wound.0), need_fill(wound.0))
                    } else {
                        String::new()
                    },
                );

                // — Traits & compétences (jauges accent) —
                let g = |label: &str, v: f32| gauge_html(label, v, &format!("{v:.2}"), "fill-accent");
                let traits_s = format!(
                    "{}{}{}{}{}{}{}{}{}",
                    g("Force", traits.strength),
                    g("Endurance", traits.endurance),
                    g("Dextérité", traits.dexterity),
                    g("Curiosité", traits.curiosity),
                    g("Sociabilité", traits.sociability),
                    g("Agressivité", traits.aggression),
                    g("Cueillette", skills.foraging),
                    g("Chasse", skills.hunting),
                    g("Oratoire", skills.oratory),
                );

                // — En ce moment : tâche, activité, prestige, portage —
                let task = beh.task.map_or_else(|| "—".to_string(), |t| task_name(t.kind));
                let mut action = format!(
                    "<div class=\"kv\"><span class=\"k\">Tâche</span><span class=\"v accent\">{task}</span></div>\
                     <div class=\"kv\"><span class=\"k\">Activité</span><span class=\"v\">{}</span></div>\
                     <div class=\"kv\"><span class=\"k\">Prestige</span><span class=\"v\">{:.1}</span></div>",
                    activity_name(beh.activity),
                    prestige.0,
                );
                if carrying.0 > 0.01 {
                    action.push_str(&format!(
                        "<div class=\"kv\"><span class=\"k\">Porte</span><span class=\"v\">{:.1} de gibier</span></div>",
                        carrying.0
                    ));
                }

                // — A vu / Sait / Lignée : puces + lignes —
                let seen: String = Exposure::ALL
                    .iter()
                    .filter(|&&e| exp.has(e))
                    .map(|&e| format!("<span class=\"chip mat\">{}</span>", exposure_name(e)))
                    .collect();
                let techs: String = know
                    .iter()
                    .map(|t| format!("<span class=\"chip tech\">{}</span>", self.sim.tech_tree.get(t).label))
                    .collect();
                let seen_block = if seen.is_empty() {
                    "<div class=\"lore-line\">A vu : rien encore</div>".to_string()
                } else {
                    format!("<div class=\"chipset\">{seen}</div>")
                };
                let tech_block = if techs.is_empty() {
                    "<div class=\"lore-line\">Sait faire : rien encore</div>".to_string()
                } else {
                    format!("<div class=\"chipset\">{techs}</div>")
                };
                // Les ascendants se nomment même s'ils sont morts depuis
                // longtemps : leur sexe est impliqué par le rôle (une mère est
                // une femme), et un nom ne dépend que de (seed, id, sexe) — donc
                // aucun besoin d'aller chercher un individu qui n'existe plus.
                let seed = self.sim.world.seed();
                let lineage = match (kin.mother, kin.father) {
                    (None, None) => "fondateur (sans ascendance)".to_string(),
                    (m, f) => format!(
                        "mère {} · père {}",
                        m.map_or("inconnue".to_string(), |a| cairn_sim::names::agent_name(
                            seed,
                            a,
                            Sex::Female
                        )),
                        f.map_or("inconnu".to_string(), |a| cairn_sim::names::agent_name(
                            seed,
                            a,
                            Sex::Male
                        )),
                    ),
                };
                let lore = format!(
                    "{seen_block}{tech_block}\
                     <div class=\"lore-line\">Lignée : {lineage}</div>\
                     <div class=\"lore-line\">Territoire : <b>{}</b> cellules · <b>{}</b> sources</div>",
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
                set_html(
                    "agent-ident",
                    "<div class=\"empty\">Cet humain n'est plus (mort, ou hors de la scène).</div>",
                );
                for id in others {
                    set_html(id, "");
                }
            }
            Some([ident, needs, traits_s, action, lore]) => {
                set_html("agent-ident", &ident);
                set_html("agent-needs", &needs);
                set_html("agent-traits", &traits_s);
                set_html("agent-action", &action);
                // `why` est None pour un nourrisson (il ne délibère pas).
                set_html(
                    "agent-why",
                    &why.unwrap_or_else(|| {
                        "<div class=\"empty\">nourrisson — porté, ne délibère pas</div>".to_string()
                    }),
                );
                set_html("agent-lore", &lore);
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
        let x_of = |i: usize| i as f64 / (n - 1) as f64 * POP_CHART_W;
        let y_of = |p: u32| POP_CHART_H - 4.0 - (f64::from(p - min_p) / span) * (POP_CHART_H - 8.0);

        // Aire sous la courbe (accent translucide).
        ctx.begin_path();
        ctx.move_to(0.0, POP_CHART_H);
        for (i, &(_, p)) in self.pop_history.iter().enumerate() {
            ctx.line_to(x_of(i), y_of(p));
        }
        ctx.line_to(POP_CHART_W, POP_CHART_H);
        ctx.close_path();
        ctx.set_fill_style_str("rgba(110, 168, 254, 0.16)");
        ctx.fill();

        // La ligne.
        ctx.begin_path();
        ctx.set_stroke_style_str("#6ea8fe");
        ctx.set_line_width(1.6);
        for (i, &(_, p)) in self.pop_history.iter().enumerate() {
            let (x, y) = (x_of(i), y_of(p));
            if i == 0 {
                ctx.move_to(x, y);
            } else {
                ctx.line_to(x, y);
            }
        }
        ctx.stroke();

        // Point de tête (dernière valeur), légèrement inséré du bord droit.
        if let Some(&(_, p)) = self.pop_history.back() {
            ctx.begin_path();
            ctx.set_fill_style_str("#bcd4ff");
            let _ = ctx.arc(POP_CHART_W - 2.0, y_of(p), 2.6, 0.0, std::f64::consts::TAU);
            ctx.fill();
        }
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
                        reveal_agent_panel();
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
        // Bouton rond du dock : l'icône seule (le libellé déborderait). Le titre
        // (infobulle) porte l'action.
        if let Some(el) = document().get_element_by_id("play-toggle") {
            el.set_text_content(Some(if self.playing { "⏸" } else { "▶" }));
            let _ = el.set_attribute("title", if self.playing { "Pause" } else { "Lecture" });
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
        Activity::Farming => "#7bc86c",                  // vert : cultive
        Activity::Fighting => "#d64545",                 // rouge sombre : combat
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

/// Couleur d'une espèce de faune sur la carte : teintes terreuses pour les
/// herbivores, froides/vives pour les prédateurs — chacune distincte, pour lire
/// « le bon animal au bon endroit ».
fn species_color(s: Species) -> &'static str {
    match s {
        Species::Deer => "#b5794a",     // cerf : brun roux
        Species::Aurochs => "#8a6a44",  // aurochs : brun sombre
        Species::Gazelle => "#d8b26a",  // gazelle : fauve clair
        Species::Reindeer => "#c3c0aa", // renne : gris-beige
        Species::Wolf => "#8b3fb0",     // loup : violet
        Species::CaveLion => "#c0562e", // lion des cavernes : rouille
    }
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

/// Écrit l'**HTML interne** d'un élément par id (les panneaux modernisés
/// composent des jauges/puces, pas du texte brut). Les valeurs injectées sont
/// toutes contrôlées (nombres, étiquettes de l'arbre tech, noms de tâches) —
/// aucune saisie libre ne transite ici, donc pas de risque d'injection.
fn set_html(id: &str, html: &str) {
    if let Some(el) = document().get_element_by_id(id) {
        el.set_inner_html(html);
    }
}

/// Une barre de jauge : un rail sombre rempli à `frac` (0–1) par la classe de
/// dégradé `fill` (`fill-good`/`warn`/`crit`/`accent`, ou vide pour le style
/// par défaut des motivations).
fn bar_html(frac: f32, fill: &str) -> String {
    format!(
        "<span class=\"bar\"><i class=\"{fill}\" style=\"width:{:.0}%\"></i></span>",
        (frac.clamp(0.0, 1.0) * 100.0)
    )
}

/// Une ligne de jauge complète : étiquette · barre · valeur.
fn gauge_html(label: &str, frac: f32, value: &str, fill: &str) -> String {
    format!(
        "<div class=\"gauge\"><span class=\"g-l\">{label}</span>{}<span class=\"g-v\">{value}</span></div>",
        bar_html(frac, fill)
    )
}

/// Couleur d'un **besoin** (faim, soif, fatigue, froid) : plus c'est haut, plus
/// c'est alarmant — vert calme, ambre à surveiller, rouge critique.
fn need_fill(v: f32) -> &'static str {
    if v < 0.4 {
        "fill-good"
    } else if v < 0.7 {
        "fill-warn"
    } else {
        "fill-crit"
    }
}

/// Couleur de la **santé** : l'inverse d'un besoin — haut = bon (vert).
fn health_fill(v: f32) -> &'static str {
    if v > 0.6 {
        "fill-good"
    } else if v > 0.3 {
        "fill-warn"
    } else {
        "fill-crit"
    }
}

/// Formate un temps réel écoulé (ms) en « mois · jours · heures · minutes »,
/// en n'affichant les grandes unités que si elles sont non nulles (mais
/// toujours au moins les minutes) — pour le « en cours depuis » des Paramètres.
fn format_elapsed(ms: f64) -> String {
    let total_min = (ms / 60_000.0).max(0.0) as u64;
    let months = total_min / (30 * 24 * 60);
    let days = (total_min / (24 * 60)) % 30;
    let hours = (total_min / 60) % 24;
    let mins = total_min % 60;
    let mut parts = Vec::new();
    if months > 0 {
        parts.push(format!("{months} mois"));
    }
    if days > 0 || months > 0 {
        parts.push(format!("{days} j"));
    }
    if hours > 0 || days > 0 || months > 0 {
        parts.push(format!("{hours} h"));
    }
    parts.push(format!("{mins} min"));
    parts.join(" · ")
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
        TaskKind::Pilgrimage => "se rendre au lieu sacré".to_string(),
        TaskKind::EatFromStock => "puiser dans le stock".to_string(),
        TaskKind::BringSurplusHome => "rapporter du gibier".to_string(),
        TaskKind::Build(k) => format!("bâtir : {}", structure_name(k)),
        TaskKind::Expedition => "expédition (chercher l'étain)".to_string(),
        TaskKind::Cultivate => "cultiver un champ".to_string(),
        TaskKind::HuntPredator => "chasser un prédateur".to_string(),
        TaskKind::Herd => "garder le cheptel".to_string(),
        TaskKind::Raid => "razzier un rival".to_string(),
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
        Activity::Farming => "cultive",
        Activity::Fighting => "combat une meute",
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

/// Déplie le panneau Agent (panneau droit) — appelé quand on sélectionne un
/// humain, pour que son inspection soit visible même si le panneau était replié.
fn reveal_agent_panel() {
    if let Some(el) = document().get_element_by_id("panel-right") {
        let _ = el.class_list().remove_1("collapsed");
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
