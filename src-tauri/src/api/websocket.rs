use std::sync::{Arc, Mutex};
use std::net::SocketAddr;
use std::collections::HashMap;
use std::thread;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tokio_tungstenite::{accept_async, WebSocketStream};
use tungstenite::Message;
use serde::{Serialize, Deserialize};
use serde_json::json;

use crate::app_state::AppState;
use crate::api::events::{Event, EventSender, set_event_sender};
use crate::models::log_entry::LogEntry;

type Tx = futures_util::stream::SplitSink<WebSocketStream<TcpStream>, Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

#[derive(Debug, Serialize, Deserialize)]
struct WebSocketCommand {
    command: String,
    args: Option<serde_json::Value>,
}

pub fn start_websocket_server(state: Arc<AppState>) {
    let peer_map = PeerMap::new(Mutex::new(HashMap::new()));

    // Create channel for events
    let (tx, rx) = std::sync::mpsc::channel::<Event>();
    set_event_sender(tx);

    // Start WebSocket server
    let peers = peer_map.clone();
    let state_clone = state.clone();

    thread::spawn(move || {
        let runtime = Runtime::new().unwrap();
        runtime.block_on(async {
            let addr = "127.0.0.1:8844";
            let listener = TcpListener::bind(&addr).await.expect("Failed to bind to WebSocket port");

            println!("WebSocket server listening on: {}", addr);

            while let Ok((stream, addr)) = listener.accept().await {
                println!("New WebSocket connection: {}", addr);
                let peers = peers.clone();
                let state = state_clone.clone();

                tokio::spawn(async move {
                    handle_connection(stream, addr, peers, state).await;
                });
            }
        });
    });

    // Start event handler
    let peers = peer_map.clone();
    thread::spawn(move || {
        for event in rx {
            broadcast_event(event, &peers);
        }
    });
}

async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    peer_map: PeerMap,
    state: Arc<AppState>
) {
    let ws_stream = accept_async(stream)
        .await
        .expect("Error during WebSocket handshake");

    let (tx, mut rx) = ws_stream.split();

    // Add new client to peer map
    peer_map.lock().unwrap().insert(addr, tx);

    // Handle incoming messages
    while let Some(msg) = rx.next().await {
        if let Ok(msg) = msg {
            match msg {
                Message::Text(text) => {
                    if let Ok(cmd) = serde_json::from_str::<WebSocketCommand>(&text) {
                        handle_command(cmd, addr, peer_map.clone(), state.clone()).await;
                    }
                },
                Message::Close(_) => {
                    break;
                },
                _ => {}
            }
        } else {
            break;
        }
    }

    // Client disconnected
    peer_map.lock().unwrap().remove(&addr);
    println!("WebSocket connection closed: {}", addr);
}

async fn handle_command(
    cmd: WebSocketCommand,
    addr: SocketAddr,
    peer_map: PeerMap,
    state: Arc<AppState>
) {
    let cmd_executor = crate::commands::command_executor::CommandExecutor::new(state.clone());

    match cmd.command.as_str() {
        "executeCommand" => {
            if let Some(args) = cmd.args {
                if let Some(command) = args.as_str() {
                    println!("Executing comman