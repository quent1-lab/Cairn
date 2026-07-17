//! Client web (WASM) de Cairn : une fenêtre sur le monde.
//!
//! Rend le monde dans un canvas plein écran, avec une caméra scrollable et
//! zoomable (monde infini). Les couches de debug (biomes, altitude,
//! température, humidité, géologie) se basculent depuis un panneau flottant.
//!
//! Choix de rendu (Phase 1) : on échantillonne directement le `worldgen`
//! (pur, local) plutôt que le store de chunks — l'observation du baseline n'a
//! pas besoin de matérialiser des tuiles mutables. L'humidité, coûteuse, tourne
//! en config rapide pour rester interactive.

// L'initialiseur du thread_local est déjà `const` ; ce lint le signale à tort
// (comportement différent wasm/natif), et un `#[allow]` sur l'item ne couvre
// pas l'expansion de la macro — d'où l'allow au niveau du crate.
#![allow(clippy::missing_const_for_thread_local)]

pub mod palette;
pub mod render;

use std::cell::RefCell;

use cairn_core::WorldSeed;
use cairn_core::scale::{km_to_tiles, tiles_to_km};
use cairn_worldgen::{HumidityConfig, WorldGen, WorldGenConfig};
use wasm_bindgen::prelude::*;
use wasm_bindgen::{Clamped, JsCast};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

use palette::Layer;

/// Zoom minimal et maximal, en pixels par tuile. Le minimum autorise une vue
/// à l'échelle continentale (~7000 km de large) : le budget d'échantillons
/// (`render::MAX_SAMPLES`) borne le coût quel que soit le dézoom, seule cette
/// constante limitait la portée. Le maximum (32 px/tuile) va jusqu'au gros
/// plan « village ».
const MIN_SCALE: f64 = 0.0004;
const MAX_SCALE: f64 = 32.0;

// État global. WASM est mono-thread : `thread_local!` + `RefCell` est
// l'idiome pour un état mutable partagé entre les closures d'événements.
thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Point d'entrée appelé automatiquement au chargement du module WASM.
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    // Renvoie les panics Rust vers la console du navigateur, lisibles.
    console_error_panic_hook::set_once();
    let app = App::new()?;
    APP.with(|slot| *slot.borrow_mut() = Some(app));
    install_event_handlers()?;
    with_app(|app| app.render());
    Ok(())
}

struct Camera {
    /// Centre de la vue, en coordonnées de tuiles (f64 pour un pan/zoom fluide).
    cx: f64,
    cy: f64,
    /// Échelle : pixels par tuile.
    scale: f64,
}

struct App {
    worldgen: WorldGen,
    ctx: CanvasRenderingContext2d,
    canvas: HtmlCanvasElement,
    width: usize,
    height: usize,
    camera: Camera,
    layer: Layer,
    /// Dernière position du curseur pendant un glisser (pan), en pixels.
    drag: Option<(f64, f64)>,
    /// Un rendu est déjà programmé pour la prochaine frame : les événements
    /// suivants ne font que mettre à jour la caméra, sans en empiler un autre.
    render_pending: bool,
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

        // Humidité en qualité « rapide » : sans diffusion latérale, moins de
        // pas — l'observation en direct privilégie la réactivité.
        let cfg = WorldGenConfig {
            humidity: HumidityConfig {
                steps: 32,
                lateral_samples: 0,
                ..HumidityConfig::default()
            },
            ..WorldGenConfig::default()
        };
        let worldgen = WorldGen::with_config(WorldSeed(42), cfg);

        let mut app = App {
            worldgen,
            ctx,
            canvas,
            width: 0,
            height: 0,
            camera: Camera {
                // Un continent tempéré de la seed 42.
                cx: km_to_tiles(1500.0),
                cy: km_to_tiles(2100.0),
                scale: 0.05,
            },
            layer: Layer::Biome,
            drag: None,
            render_pending: false,
        };
        app.resize();
        Ok(app)
    }

    /// Ajuste la taille du canvas à la fenêtre (device pixels).
    fn resize(&mut self) {
        let win = window();
        let w = win.inner_width().unwrap().as_f64().unwrap() as usize;
        let h = win.inner_height().unwrap().as_f64().unwrap() as usize;
        self.width = w.max(1);
        self.height = h.max(1);
        self.canvas.set_width(self.width as u32);
        self.canvas.set_height(self.height as u32);
    }

    /// Coordonnée de tuile (f64) sous un pixel de l'écran.
    fn tile_at(&self, px: f64, py: f64) -> (f64, f64) {
        render::tile_at(
            px,
            py,
            self.camera.cx,
            self.camera.cy,
            self.camera.scale,
            self.width,
            self.height,
        )
    }

    fn render(&self) {
        let buf = render::render_to_buffer(
            &self.worldgen,
            self.camera.cx,
            self.camera.cy,
            self.camera.scale,
            self.layer,
            self.width,
            self.height,
        );
        let img = ImageData::new_with_u8_clamped_array_and_sh(
            Clamped(&buf),
            self.width as u32,
            self.height as u32,
        )
        .expect("ImageData");
        self.ctx.put_image_data(&img, 0.0, 0.0).expect("put_image_data");
        self.update_readout();
    }

    /// Programme un rendu pour la prochaine frame plutôt que de le faire tout
    /// de suite. Sans ça, un glisser rapide déclenche bien plus de `mousemove`
    /// qu'un rendu ne peut en absorber : ils s'empilent et la carte « traîne »
    /// derrière le curseur. Ici on *coalesce* — quel que soit le nombre
    /// d'événements, un seul rendu par frame, avec la caméra la plus à jour.
    fn request_render(&mut self) {
        if self.render_pending {
            return;
        }
        self.render_pending = true;
        // `once_into_js` : la closure ne sera appelée qu'une fois (rAF ne
        // rappelle pas), puis libérée automatiquement par le shim wasm-bindgen
        // — pas de `forget()` qui fuirait une closure par frame. Elle ne
        // capture rien qui emprunte APP : le rendu passe par `with_app`, exécuté
        // plus tard, une fois cet emprunt-ci relâché.
        let cb = Closure::once_into_js(move || {
            with_app(|a| {
                a.render_pending = false;
                a.render();
            });
        });
        window()
            .request_animation_frame(cb.unchecked_ref())
            .expect("request_animation_frame");
    }

    /// Met à jour le bandeau d'information (coordonnées, échelle, couche).
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
    }

    fn on_pointer_down(&mut self, px: f64, py: f64) {
        self.drag = Some((px, py));
    }

    fn on_pointer_move(&mut self, px: f64, py: f64) {
        if let Some((lx, ly)) = self.drag {
            // Glisser la carte déplace le monde en sens inverse.
            self.camera.cx -= (px - lx) / self.camera.scale;
            self.camera.cy -= (py - ly) / self.camera.scale;
            self.drag = Some((px, py));
            self.request_render();
        }
    }

    fn on_pointer_up(&mut self) {
        self.drag = None;
    }

    /// Zoom géométrique centré sur le curseur : le point du monde sous la
    /// souris reste fixe.
    fn on_wheel(&mut self, delta_y: f64, mx: f64, my: f64) {
        let (wx, wy) = self.tile_at(mx, my);
        let factor = (-delta_y * 0.0015).exp();
        self.camera.scale = (self.camera.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        // Recale le centre pour que (wx, wy) retombe sous (mx, my).
        self.camera.cx = wx - (mx - self.width as f64 / 2.0) / self.camera.scale;
        self.camera.cy = wy - (my - self.height as f64 / 2.0) / self.camera.scale;
        self.request_render();
    }

    fn on_resize(&mut self) {
        self.resize();
        self.request_render();
    }

    fn set_layer(&mut self, layer: Layer) {
        self.layer = layer;
        self.request_render();
    }
}

// ─────────────────────────── câblage des événements ───────────────────────────

/// Ajoute un écouteur typé et **fuit** la closure (elle doit vivre aussi
/// longtemps que la page — pas de désinscription en Phase 1).
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

    // Souris : down sur le canvas, move/up sur la fenêtre (le glisser continue
    // même si le curseur sort du canvas).
    listen!(canvas, "mousedown", web_sys::MouseEvent, move |e: web_sys::MouseEvent| {
        with_app(|a| a.on_pointer_down(e.client_x() as f64, e.client_y() as f64));
    });
    listen!(win, "mousemove", web_sys::MouseEvent, move |e: web_sys::MouseEvent| {
        with_app(|a| a.on_pointer_move(e.client_x() as f64, e.client_y() as f64));
    });
    listen!(win, "mouseup", web_sys::MouseEvent, move |_e: web_sys::MouseEvent| {
        with_app(App::on_pointer_up);
    });
    listen!(canvas, "wheel", web_sys::WheelEvent, move |e: web_sys::WheelEvent| {
        e.prevent_default();
        with_app(|a| a.on_wheel(e.delta_y(), e.client_x() as f64, e.client_y() as f64));
    });
    listen!(win, "resize", web_sys::Event, move |_e: web_sys::Event| {
        with_app(App::on_resize);
    });

    // Boutons de couche du panneau flottant.
    let buttons = document().query_selector_all(".layer-btn")?;
    for i in 0..buttons.length() {
        let btn = buttons.item(i).unwrap().dyn_into::<web_sys::HtmlElement>()?;
        let id = btn.get_attribute("data-layer").unwrap_or_default();
        listen!(btn, "click", web_sys::MouseEvent, move |_e: web_sys::MouseEvent| {
            if let Some(layer) = Layer::from_id(&id) {
                set_active_button(&id);
                with_app(|a| a.set_layer(layer));
            }
        });
    }
    set_active_button(Layer::Biome.id());
    Ok(())
}

/// Applique la classe `active` au bouton de la couche choisie.
fn set_active_button(id: &str) {
    if let Ok(buttons) = document().query_selector_all(".layer-btn") {
        for i in 0..buttons.length() {
            if let Some(el) = buttons.item(i).and_then(|n| n.dyn_into::<web_sys::Element>().ok()) {
                let is_active = el.get_attribute("data-layer").as_deref() == Some(id);
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
