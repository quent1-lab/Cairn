//! Le mode **observateur** (`?live=1`) : le monde ne tourne plus dans
//! l'onglet, il tourne sur le serveur (`cairn-server`) ; le client n'en reçoit
//! que ce qui se dessine.
//!
//! Ce que le fil apporte, et ce qu'il n'apporte pas :
//! - la **seed**, une fois ([`Hello`]) : le terrain est régénéré ici, aucun
//!   octet de tuile ne transite ;
//! - l'**instantané** de la fenêtre regardée, à 5 Hz ([`WorldSnapshot`]) ; on
//!   interpole les positions entre les deux derniers pour que le mouvement
//!   reste continu.
//!
//! Le panneau d'inspection, les gestes divins et la Chronique restent au mode
//! local tant que le protocole ne les porte pas (incréments suivants de SRV).

use std::cell::RefCell;
use std::collections::BTreeMap;

use cairn_core::WorldSeed;
use cairn_core::scale::tiles_to_km;
use cairn_core::time::SimTime;
use cairn_protocol::{ClientMsg, FaunaRole, PROTOCOL_VERSION, Region, ServerMsg, WorldSnapshot};
use cairn_worldgen::WorldGen;
use wasm_bindgen::prelude::*;
use wasm_bindgen::{Clamped, JsCast};
use web_sys::{BinaryType, CanvasRenderingContext2d, HtmlCanvasElement, ImageData, MessageEvent, WebSocket};

use crate::palette::Layer;
use crate::{
    MAX_SCALE, MIN_SCALE, activity_color, clan_color_u64, document, render, set_display, set_html,
    set_text, species_color, structure_color, structure_offset, window,
};

/// Période des instantanés côté serveur (5 Hz) : sert à l'interpolation.
const FRAME_MS: f64 = 200.0;
/// Marge autour de l'écran, en fraction de sa largeur : on demande un peu
/// plus que ce qu'on voit, pour qu'un petit déplacement ne laisse pas un bord
/// vide le temps qu'arrive l'instantané suivant.
const REGION_MARGIN: f64 = 0.25;

thread_local! {
    static LIVE: RefCell<Option<Live>> = const { RefCell::new(None) };
}

struct Live {
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    socket: WebSocket,
    width: usize,
    height: usize,
    cx: f64,
    cy: f64,
    scale: f64,
    drag: Option<(f64, f64)>,
    /// Le terrain n'existe qu'une fois la seed connue.
    worldgen: Option<WorldGen>,
    terrain: Vec<u8>,
    terrain_valid: bool,
    snapshot: Option<WorldSnapshot>,
    /// Positions de l'instantané précédent, par identifiant : le point de
    /// départ de l'interpolation.
    prev_agents: BTreeMap<u64, (f64, f64)>,
    prev_fauna: BTreeMap<u64, (f64, f64)>,
    received_ms: f64,
    /// La caméra ne s'est pas encore posée sur le peuple.
    framed: bool,
    subscribed: Option<Region>,
    status: String,
}

fn with_live(f: impl FnOnce(&mut Live)) {
    LIVE.with(|slot| {
        if let Some(l) = slot.borrow_mut().as_mut() {
            f(l);
        }
    });
}

fn now_ms() -> f64 {
    window().performance().map(|p| p.now()).unwrap_or(0.0)
}

/// L'adresse du serveur : `?server=hôte:port`, sinon celui qui a servi la page.
fn server_url() -> String {
    let loc = window().location();
    let search = loc.search().unwrap_or_default();
    let host = search
        .trim_start_matches('?')
        .split('&')
        .find_map(|kv| kv.strip_prefix("server="))
        .map(str::to_owned)
        .unwrap_or_else(|| loc.host().unwrap_or_default());
    let secure = loc.protocol().map(|p| p == "https:").unwrap_or(false);
    format!("{}://{host}/ws", if secure { "wss" } else { "ws" })
}

pub fn start() -> Result<(), JsValue> {
    let canvas = document()
        .get_element_by_id("view")
        .ok_or("canvas #view introuvable")?
        .dyn_into::<HtmlCanvasElement>()?;
    let ctx = canvas
        .get_context("2d")?
        .ok_or("contexte 2d indisponible")?
        .dyn_into::<CanvasRenderingContext2d>()?;
    // Ce qui suppose une simulation locale n'a pas de sens ici.
    for id in ["panel-right", "panel-params"] {
        set_display(id, false);
    }

    let socket = WebSocket::new(&server_url())?;
    socket.set_binary_type(BinaryType::Arraybuffer);
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(|e: MessageEvent| {
        if let Ok(buf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
            let bytes = js_sys::Uint8Array::new(&buf).to_vec();
            with_live(|l| l.on_message(&bytes));
        }
    });
    socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();
    let on_close = Closure::<dyn FnMut(JsValue)>::new(|_| {
        with_live(|l| l.status = "connexion perdue — rechargez la page".into());
    });
    socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));
    on_close.forget();

    let mut live = Live {
        canvas,
        ctx,
        socket,
        width: 0,
        height: 0,
        cx: 0.0,
        cy: 0.0,
        scale: 2.0,
        drag: None,
        worldgen: None,
        terrain: Vec::new(),
        terrain_valid: false,
        snapshot: None,
        prev_agents: BTreeMap::new(),
        prev_fauna: BTreeMap::new(),
        received_ms: 0.0,
        framed: false,
        subscribed: None,
        status: "connexion au serveur…".into(),
    };
    live.resize();
    LIVE.with(|slot| *slot.borrow_mut() = Some(live));
    install_handlers()?;
    schedule_frame();
    Ok(())
}

impl Live {
    fn resize(&mut self) {
        let win = window();
        let w = win.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(800.0) as usize;
        let h = win.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(600.0) as usize;
        self.width = w.max(1);
        self.height = h.max(1);
        self.canvas.set_width(self.width as u32);
        self.canvas.set_height(self.height as u32);
        self.terrain_valid = false;
    }

    fn on_message(&mut self, bytes: &[u8]) {
        let Ok(msg) = cairn_protocol::decode::<ServerMsg>(bytes) else {
            self.status = "message illisible — client et serveur de versions différentes ?".into();
            return;
        };
        match msg {
            ServerMsg::Hello(h) => {
                if h.protocol != PROTOCOL_VERSION {
                    self.status = format!(
                        "protocole {} attendu, le serveur parle {} — rechargez la page",
                        PROTOCOL_VERSION, h.protocol
                    );
                    let _ = self.socket.close();
                    return;
                }
                // La même génération que le serveur (`Sim::new`, configuration
                // par défaut) : sinon on peindrait un autre monde sous les pas.
                self.worldgen = Some(WorldGen::new(WorldSeed(h.seed)));
                self.terrain_valid = false;
                self.status = format!("connecté · seed {}", h.seed);
                // Avant de savoir où est le peuple, on regarde tout le monde
                // pour le trouver ; la caméra se posera au premier instantané.
                self.send_subscribe(Region::everything());
            }
            ServerMsg::Snapshot(snap) => {
                self.prev_agents.clear();
                self.prev_fauna.clear();
                if let Some(old) = &self.snapshot {
                    self.prev_agents.extend(old.agents.iter().map(|a| (a.id, (a.x, a.y))));
                    self.prev_fauna.extend(old.fauna.iter().map(|f| (f.id, (f.x, f.y))));
                }
                self.received_ms = now_ms();
                if !self.framed {
                    self.frame_on(&snap);
                }
                self.snapshot = Some(*snap);
            }
            ServerMsg::Refused(why) => self.status = format!("refusé : {why}"),
            ServerMsg::Chronicle(_) | ServerMsg::MiracleResult(_) => {}
        }
    }

    /// Pose la caméra sur le peuple (le premier foyer, à défaut le premier
    /// humain), puis restreint l'abonnement à ce qu'on voit.
    fn frame_on(&mut self, snap: &WorldSnapshot) {
        let target = snap
            .clans
            .first()
            .map(|c| (c.home_x, c.home_y))
            .or_else(|| snap.agents.first().map(|a| (a.x, a.y)));
        if let Some((x, y)) = target {
            self.cx = x;
            self.cy = y;
            self.framed = true;
            self.terrain_valid = false;
        }
    }

    fn send_subscribe(&mut self, region: Region) {
        if self.socket.ready_state() != WebSocket::OPEN || self.subscribed == Some(region) {
            return;
        }
        if let Ok(bytes) = cairn_protocol::encode(&ClientMsg::Subscribe { region }) {
            let _ = self.socket.send_with_u8_array(&bytes);
            self.subscribed = Some(region);
        }
    }

    /// La fenêtre visible, élargie de la marge. On ne renvoie l'abonnement
    /// que si l'écran est sorti de la fenêtre déjà demandée.
    fn update_subscription(&mut self) {
        if !self.framed {
            return;
        }
        let half_w = self.width as f64 / 2.0 / self.scale;
        let half_h = self.height as f64 / 2.0 / self.scale;
        let seen = Region {
            min_x: (self.cx - half_w) as i64,
            min_y: (self.cy - half_h) as i64,
            max_x: (self.cx + half_w) as i64,
            max_y: (self.cy + half_h) as i64,
        };
        let inside = self.subscribed.is_some_and(|r| {
            r != Region::everything()
                && r.min_x <= seen.min_x
                && r.min_y <= seen.min_y
                && r.max_x >= seen.max_x
                && r.max_y >= seen.max_y
                // Un zoom avant laisse l'écran dans la fenêtre : on la resserre
                // quand même dès qu'elle est trop grande pour ce qu'on voit.
                && (r.max_x - r.min_x) as f64 <= 4.0 * half_w * (1.0 + REGION_MARGIN)
        });
        if inside {
            return;
        }
        let m = 1.0 + REGION_MARGIN;
        self.send_subscribe(Region::around(
            (self.cx, self.cy),
            (half_w * m).ceil() as i64,
            (half_h * m).ceil() as i64,
        ));
    }

    fn frame(&mut self) {
        self.update_subscription();
        if let (Some(wg), false) = (&self.worldgen, self.terrain_valid) {
            self.terrain = render::render_to_buffer(
                wg,
                self.cx,
                self.cy,
                self.scale,
                Layer::Biome,
                self.width,
                self.height,
            );
            self.terrain_valid = true;
        }
        if self.terrain_valid {
            if let Ok(img) = ImageData::new_with_u8_clamped_array_and_sh(
                Clamped(&self.terrain),
                self.width as u32,
                self.height as u32,
            ) {
                let _ = self.ctx.put_image_data(&img, 0.0, 0.0);
            }
        } else {
            self.ctx.set_fill_style_str("#0b0d12");
            self.ctx.fill_rect(0.0, 0.0, self.width as f64, self.height as f64);
        }
        self.draw_entities();
        self.update_readout();
    }

    fn draw_entities(&self) {
        let Some(snap) = &self.snapshot else { return };
        let (w, h) = (self.width as f64, self.height as f64);
        let (cx, cy, scale) = (self.cx, self.cy, self.scale);
        let a = ((now_ms() - self.received_ms) / FRAME_MS).clamp(0.0, 1.0);
        let to_screen = |x: f64, y: f64| ((x - cx) * scale + w / 2.0, (y - cy) * scale + h / 2.0);
        let place = |prev: Option<&(f64, f64)>, x: f64, y: f64| {
            let (px, py) = prev.copied().unwrap_or((x, y));
            to_screen(px + (x - px) * a, py + (y - py) * a)
        };
        let ctx = &self.ctx;
        let tau = std::f64::consts::TAU;

        // Territoires et foyers : le même fond que le mode local.
        let terr_r = cairn_sim::social::RESIDENCE_RADIUS_TILES * scale;
        ctx.set_line_width(1.0);
        ctx.set_global_alpha(0.28);
        for clan in &snap.clans {
            let (sx, sy) = to_screen(clan.home_x, clan.home_y);
            ctx.set_stroke_style_str(&clan_color_u64(clan.id));
            ctx.begin_path();
            let _ = ctx.arc(sx, sy, terr_r, 0.0, tau);
            ctx.stroke();
        }
        ctx.set_global_alpha(1.0);
        ctx.set_stroke_style_str("#e0503a");
        for &(ia, ib, t) in &snap.tensions {
            if t < 0.05 {
                continue;
            }
            let home = |id: u64| snap.clans.iter().find(|c| c.id == id).map(|c| (c.home_x, c.home_y));
            let (Some(pa), Some(pb)) = (home(ia), home(ib)) else { continue };
            let (ax, ay) = to_screen(pa.0, pa.1);
            let (bx, by) = to_screen(pb.0, pb.1);
            ctx.set_global_alpha(f64::from(t).clamp(0.2, 0.9));
            ctx.set_line_width(1.0 + 3.0 * f64::from(t));
            ctx.begin_path();
            ctx.move_to(ax, ay);
            ctx.line_to(bx, by);
            ctx.stroke();
        }
        ctx.set_global_alpha(1.0);
        ctx.set_line_width(2.0);
        for clan in &snap.clans {
            let (sx, sy) = to_screen(clan.home_x, clan.home_y);
            ctx.set_stroke_style_str(&clan_color_u64(clan.id));
            ctx.begin_path();
            let _ = ctx.arc(sx, sy, (scale * 6.0).clamp(6.0, 40.0), 0.0, tau);
            ctx.stroke();
        }
        for st in &snap.structures {
            let (bx, by) = to_screen(st.x, st.y);
            let (ox, oy) = structure_offset(st.kind);
            let sz = (scale * 4.0).clamp(5.0, 12.0);
            ctx.set_global_alpha(if st.abandoned { 0.4 } else { 1.0 });
            ctx.set_fill_style_str(structure_color(st.kind));
            ctx.fill_rect(bx + ox - sz / 2.0, by + oy - sz / 2.0, sz, sz);
        }
        // Feux et météo : des voiles translucides.
        for cell in &snap.weather {
            let (sx, sy) = to_screen(cell.x, cell.y);
            let (alpha, color) = match cell.kind {
                None => (0.4, "#ff6a2a"),
                Some(cairn_sim::WeatherKind::Rain) => (0.14, "#5aa9ff"),
                Some(_) => (0.14, "#d8a24a"),
            };
            ctx.set_global_alpha(alpha);
            ctx.set_fill_style_str(color);
            ctx.begin_path();
            let _ = ctx.arc(sx, sy, (cell.radius * scale).max(2.0), 0.0, tau);
            ctx.fill();
        }
        ctx.set_global_alpha(0.95);
        ctx.set_fill_style_str("#ded0ff");
        for &(x, y) in &snap.shrines {
            let (sx, sy) = to_screen(x, y);
            ctx.begin_path();
            ctx.move_to(sx, sy - 5.0);
            ctx.line_to(sx + 5.0, sy);
            ctx.line_to(sx, sy + 5.0);
            ctx.line_to(sx - 5.0, sy);
            ctx.close_path();
            ctx.fill();
        }
        ctx.set_global_alpha(1.0);

        // La faune, puis les humains par-dessus : un carré par groupe, un
        // carré par être, à la taille de l'âge.
        for f in &snap.fauna {
            let (sx, sy) = place(self.prev_fauna.get(&f.id), f.x, f.y);
            let sz = match f.role {
                FaunaRole::Herd => ((scale * 1.5) * (1.0 + f64::from(f.population) / 60.0)).clamp(3.0, 16.0),
                FaunaRole::Pack => ((scale * 1.5) * (1.0 + f64::from(f.population) / 8.0)).clamp(3.0, 12.0),
            };
            ctx.set_fill_style_str(species_color(f.species));
            ctx.fill_rect(sx - sz / 2.0, sy - sz / 2.0, sz, sz);
        }
        let s = scale.clamp(2.5, 8.0);
        for ag in &snap.agents {
            let (sx, sy) = place(self.prev_agents.get(&ag.id), ag.x, ag.y);
            let adult = ag.age_years >= 14.0;
            let sz = if adult { s } else { (s * 0.55).max(2.0) };
            ctx.set_fill_style_str(activity_color(ag.activity));
            ctx.fill_rect(sx - sz / 2.0, sy - sz / 2.0, sz, sz);
        }
    }

    fn update_readout(&self) {
        set_text(
            "readout",
            &format!(
                "{} · centre ({:.0}, {:.0}) km · largeur {:.0} km",
                self.status,
                tiles_to_km(self.cx),
                tiles_to_km(self.cy),
                tiles_to_km(self.width as f64 / self.scale),
            ),
        );
        let Some(snap) = &self.snapshot else { return };
        let s = &snap.status;
        let t = SimTime { tick: s.tick };
        set_html("dock-date", &format!("An <b>{}</b> · jour <b>{}</b>", t.year(), t.day_of_year()));
        set_text("faith-value", &format!("{:.0}", s.faith));
        set_text("sim-readout", &format!("{:.0} gibier · {:.0} prédateurs", s.herbivores, s.predators));
        set_html(
            "pop-stats",
            &format!(
                "<div>{} humains ({} ♀, {} ♂, {} enfants) · {} naissances, {} morts · {} clans · {} visibles</div>",
                s.population,
                s.females,
                s.males,
                s.children,
                s.births,
                s.deaths,
                snap.clans.len(),
                snap.agents.len(),
            ),
        );
    }

    fn on_wheel(&mut self, delta_y: f64, mx: f64, my: f64) {
        let (wx, wy) = render::tile_at(mx, my, self.cx, self.cy, self.scale, self.width, self.height);
        self.scale = (self.scale * (-delta_y * 0.0015).exp()).clamp(MIN_SCALE, MAX_SCALE);
        self.cx = wx - (mx - self.width as f64 / 2.0) / self.scale;
        self.cy = wy - (my - self.height as f64 / 2.0) / self.scale;
        self.terrain_valid = false;
    }

    fn on_move(&mut self, px: f64, py: f64) {
        if let Some((lx, ly)) = self.drag {
            self.cx -= (px - lx) / self.scale;
            self.cy -= (py - ly) / self.scale;
            self.drag = Some((px, py));
            self.terrain_valid = false;
        }
    }
}

fn schedule_frame() {
    let cb = Closure::once_into_js(move |_ts: f64| {
        with_live(|l| l.frame());
        schedule_frame();
    });
    let _ = window().request_animation_frame(cb.unchecked_ref());
}

fn install_handlers() -> Result<(), JsValue> {
    let canvas = document().get_element_by_id("view").ok_or("canvas #view introuvable")?;
    let win = window();
    let down = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(|e: web_sys::MouseEvent| {
        with_live(|l| l.drag = Some((e.client_x() as f64, e.client_y() as f64)));
    });
    canvas.add_event_listener_with_callback("mousedown", down.as_ref().unchecked_ref())?;
    down.forget();
    let mv = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(|e: web_sys::MouseEvent| {
        with_live(|l| l.on_move(e.client_x() as f64, e.client_y() as f64));
    });
    win.add_event_listener_with_callback("mousemove", mv.as_ref().unchecked_ref())?;
    mv.forget();
    let up = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(|_e: web_sys::MouseEvent| {
        with_live(|l| l.drag = None);
    });
    win.add_event_listener_with_callback("mouseup", up.as_ref().unchecked_ref())?;
    up.forget();
    let wheel = Closure::<dyn FnMut(web_sys::WheelEvent)>::new(|e: web_sys::WheelEvent| {
        e.prevent_default();
        with_live(|l| l.on_wheel(e.delta_y(), e.client_x() as f64, e.client_y() as f64));
    });
    canvas.add_event_listener_with_callback("wheel", wheel.as_ref().unchecked_ref())?;
    wheel.forget();
    let resize = Closure::<dyn FnMut(web_sys::Event)>::new(|_e: web_sys::Event| {
        with_live(|l| l.resize());
    });
    win.add_event_listener_with_callback("resize", resize.as_ref().unchecked_ref())?;
    resize.forget();
    Ok(())
}
