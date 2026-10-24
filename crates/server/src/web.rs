//! Le côté réseau : servir le client (fichiers statiques) et tenir une
//! connexion WebSocket par observateur.

use std::path::{Component, Path};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use cairn_protocol::{ClientMsg, Hello, PROTOCOL_VERSION, ServerMsg};

use crate::Shared;

pub async fn serve(shared: Arc<Shared>, port: u16) {
    let app = Router::new()
        .route("/ws", get(ws_upgrade))
        .fallback(get(static_file))
        .with_state(shared);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.expect("port occupé");
    eprintln!("Cairn servi sur http://localhost:{port}/?live=1");
    axum::serve(listener, app).await.expect("serveur");
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(shared): State<Arc<Shared>>) -> Response {
    ws.on_upgrade(move |socket| observe(socket, shared))
}

/// La vie d'une connexion : on dit de quel monde il s'agit, puis on relaie
/// les instantanés de la fenêtre que le client demande, jusqu'à ce qu'il parte.
async fn observe(mut socket: WebSocket, shared: Arc<Shared>) {
    let hello = ServerMsg::Hello(Hello {
        protocol: PROTOCOL_VERSION,
        seed: shared.seed,
        tick: shared.tick.load(Ordering::Relaxed),
    });
    let bytes = cairn_protocol::encode(&hello).expect("Hello s'encode");
    if socket.send(Message::Binary(bytes.into())).await.is_err() {
        return;
    }

    let (id, mut frames) = shared.hub.join();
    loop {
        tokio::select! {
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Binary(b))) => match cairn_protocol::decode::<ClientMsg>(&b) {
                    Ok(ClientMsg::Subscribe { region }) => shared.hub.subscribe(id, region),
                    // Les gestes divins et la Chronique passent par le fil aux
                    // incréments suivants (SRV-1 ne fait qu'observer).
                    Ok(_) => {}
                    Err(_) => break,
                },
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
            changed = frames.changed() => {
                if changed.is_err() {
                    break;
                }
                let frame = frames.borrow_and_update().clone();
                if let Some(frame) = frame {
                    if socket.send(Message::Binary(frame.to_vec().into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
    shared.hub.leave(id);
}

/// Sert le bundle du client (`build.sh`). `.wasm` doit partir en
/// `application/wasm`, sinon le navigateur refuse de l'instancier.
async fn static_file(State(shared): State<Arc<Shared>>, uri: Uri) -> Response {
    let rel = uri.path().trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    let rel = Path::new(rel);
    // Anti-traversée : rien que des noms, jamais `..` ni racine.
    if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return StatusCode::FORBIDDEN.into_response();
    }
    // Lecture bloquante : quelques fichiers de quelques centaines de Ko, lus
    // au chargement de la page — pas de quoi gêner le relais des instantanés.
    match std::fs::read(shared.client_dir.join(rel)) {
        Ok(body) => ([(header::CONTENT_TYPE, mime(rel))], body).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css; charset=utf-8",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    }
}
