use crate::application::{
    session_manager, ControllerManager, Session, SharedSession, SourceCategory, TargetCategory,
};
use crate::core::when;
use crate::domain::MappingCompartment;
use crate::infrastructure::data::ControllerData;
use crate::infrastructure::plugin::App;
use futures::channel::oneshot;
use futures::SinkExt;
use rcgen::{Certificate, CertificateParams, SanType};
use reaper_high::Reaper;
use rxrust::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::fmt::Debug;
use std::fs;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{JoinHandle, Thread};
use tokio::sync::{mpsc, RwLock};
use warp::http::{Response, StatusCode};
use warp::reject::Reject;
use warp::reply::Json;
use warp::ws::{Message, WebSocket};
use warp::{reject, reply, Rejection, Reply};

pub type SharedRealearnServer = Rc<RefCell<RealearnServer>>;

pub struct RealearnServer {
    port: u16,
    state: ServerState,
    cert_dir_path: PathBuf,
}

enum ServerState {
    Stopped,
    Started {
        clients: ServerClients,
        shutdown_sender: oneshot::Sender<()>,
    },
}

impl RealearnServer {
    pub fn new(port: u16, cert_dir_path: PathBuf) -> RealearnServer {
        RealearnServer {
            port,
            state: ServerState::Stopped,
            cert_dir_path,
        }
    }

    /// Idempotent
    pub fn start(&mut self) {
        if let ServerState::Started { .. } = self.state {
            return;
        }
        let clients: ServerClients = Default::default();
        let clients_clone = clients.clone();
        let ip = self.local_ip();
        let port = self.port;
        let cert_dir_path = self.cert_dir_path.clone();
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        std::thread::Builder::new()
            .name("ReaLearn server".to_string())
            .spawn(move || {
                let mut runtime = tokio::runtime::Builder::new()
                    .basic_scheduler()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime.block_on(start_server(
                    port,
                    clients_clone,
                    ip,
                    &cert_dir_path,
                    shutdown_receiver,
                ));
            });
        self.state = ServerState::Started {
            clients,
            shutdown_sender,
        };
    }

    /// Idempotent
    pub fn stop(&mut self) {
        let old_state = std::mem::replace(&mut self.state, ServerState::Stopped);
        let mut shutdown_sender = match old_state {
            ServerState::Started {
                shutdown_sender, ..
            } => shutdown_sender,
            ServerState::Stopped => return,
        };
        shutdown_sender.send(());
    }

    pub fn clients(&self) -> Result<&ServerClients, &'static str> {
        if let ServerState::Started { clients, .. } = &self.state {
            Ok(clients)
        } else {
            Err("server not running")
        }
    }

    pub fn is_running(&self) -> bool {
        if let ServerState::Started { .. } = &self.state {
            true
        } else {
            false
        }
    }

    pub fn generate_realearn_app_url(&self, session_id: &str) -> String {
        let host = self
            .local_ip()
            .map(|ip| ip.to_string())
            .unwrap_or("127.0.0.1".to_string());
        format!(
            "https://www.helgoboss.org/projects/realearn/app/#{}:{}/{}",
            host,
            self.port(),
            session_id
        )
    }

    pub fn local_ip(&self) -> Option<IpAddr> {
        get_local_ip()
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn log_debug_info(&self, session_id: &str) {
        let msg = format!(
            "\n\
        # Server\n\
        \n\
        - ReaLearn app URL: {}
        ",
            self.generate_realearn_app_url(session_id)
        );
        Reaper::get().show_console_msg(msg);
    }
}

static NEXT_CLIENT_ID: AtomicUsize = AtomicUsize::new(1);

async fn in_main_thread<O: Reply + 'static, E: Reply + 'static>(
    op: impl FnOnce() -> Result<O, E> + 'static,
) -> Result<Box<dyn Reply>, Rejection> {
    let send_result = Reaper::get().main_thread_future(move || op()).await;
    let response_result = match send_result {
        Ok(r) => r,
        Err(_) => {
            return Ok(Box::new(
                Response::builder()
                    .status(500)
                    .body("sender dropped")
                    .unwrap(),
            ));
        }
    };
    let raw: Box<dyn Reply> = match response_result {
        Ok(r) => Box::new(r),
        Err(r) => Box::new(r),
    };
    Ok(raw)
}

fn handle_controller_routing_route(session_id: String) -> Result<Json, Response<&'static str>> {
    let session = session_manager::find_session_by_id(&session_id).ok_or_else(session_not_found)?;
    let routing = get_controller_routing(&session.borrow());
    Ok(reply::json(&routing))
}

fn handle_patch_controller_route(
    controller_id: String,
    req: PatchRequest,
) -> Result<StatusCode, Response<&'static str>> {
    if req.op != PatchRequestOp::Replace {
        return Err(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body("only 'replace' is supported as op")
            .unwrap());
    }
    let split_path: Vec<_> = req.path.split('/').collect();
    let custom_data_key = if let ["", "customData", key] = split_path.as_slice() {
        key
    } else {
        return Err(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("only '/customData/{key}' is supported as path")
            .unwrap());
    };
    let controller_manager = App::get().controller_manager();
    let mut controller_manager = controller_manager.borrow_mut();
    let mut controller = controller_manager
        .find_by_id(&controller_id)
        .ok_or_else(controller_not_found)?;
    controller.update_custom_data(custom_data_key.to_string(), req.value);
    controller_manager
        .update_controller(controller)
        .map_err(|_| internal_server_error("couldn't update controller"))?;
    Ok(StatusCode::OK)
}

fn session_not_found() -> Response<&'static str> {
    not_found("session not found")
}

fn session_has_no_active_controller() -> Response<&'static str> {
    not_found("session doesn't have an active controller")
}

fn controller_not_found() -> Response<&'static str> {
    not_found("session has controller but controller not found")
}

fn not_found(msg: &'static str) -> Response<&'static str> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(msg)
        .unwrap()
}

fn internal_server_error(msg: &'static str) -> Response<&'static str> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(msg)
        .unwrap()
}

fn bad_request(msg: &'static str) -> Response<&'static str> {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body(msg)
        .unwrap()
}

fn handle_controller_route(session_id: String) -> Result<Json, Response<&'static str>> {
    let session = session_manager::find_session_by_id(&session_id).ok_or_else(session_not_found)?;
    let session = session.borrow();
    let controller_id = session
        .active_controller_id()
        .ok_or_else(session_has_no_active_controller)?;
    let controller = App::get()
        .controller_manager()
        .borrow()
        .find_by_id(controller_id)
        .ok_or_else(controller_not_found)?;
    let controller_data = ControllerData::from_model(&controller);
    Ok(reply::json(&controller_data))
}

async fn start_server(
    port: u16,
    clients: ServerClients,
    ip: Option<IpAddr>,
    cert_dir_path: &Path,
    shutdown_receiver: oneshot::Receiver<()>,
) {
    use warp::Filter;
    let controller_route = warp::get()
        .and(warp::path!("realearn" / "session" / String / "controller"))
        .and_then(|session_id| in_main_thread(|| handle_controller_route(session_id)));
    let controller_routing_route = warp::get()
        .and(warp::path!(
            "realearn" / "session" / String / "controller-routing"
        ))
        .and_then(|session_id| in_main_thread(|| handle_controller_routing_route(session_id)));
    let patch_controller_route = warp::patch()
        .and(warp::path!("realearn" / "controller" / String))
        .and(warp::body::json())
        .and_then(|controller_id, req: PatchRequest| {
            in_main_thread(|| handle_patch_controller_route(controller_id, req))
        });
    let ws_route = {
        let clients = warp::any().map(move || clients.clone());
        warp::path::end()
            .and(warp::ws())
            .and(warp::query::<WebSocketRequest>())
            .and(clients)
            .map(
                |ws: warp::ws::Ws, req: WebSocketRequest, clients| -> Box<dyn Reply> {
                    let topics: Result<HashSet<_>, _> =
                        req.topics.split(',').map(Topic::try_from).collect();
                    if let Ok(topics) = topics {
                        Box::new(ws.on_upgrade(move |ws| client_connected(ws, topics, clients)))
                    } else {
                        Box::new(bad_request("at least one of the given topics is invalid"))
                    }
                },
            )
    };
    let routes = controller_route
        .or(controller_routing_route)
        .or(patch_controller_route)
        .or(ws_route);
    if let Some(ip) = ip {
        let (key, cert) = get_key_and_cert(ip, cert_dir_path);
        let server = warp::serve(routes).tls().key(key).cert(cert);
        server
            .bind_with_graceful_shutdown(([0, 0, 0, 0], port), async {
                shutdown_receiver.await.unwrap()
            })
            .1
            .await;
    } else {
        // TODO-high In this case we should consider binding to localhost only and give a
        //  warning message on "Projection" screen. Or well, actually we can still create
        //  a self-signed certificate and just ignore the host name mismatch on the client.
        //  On Flutter, this works using badCertificateCallback. On the web it doesn't make
        //  a real difference. Using a self-signed certificate will have to be manually accepted,
        //  no matter if the host name is correct or not.
        let server = warp::serve(routes);
        server
            .bind_with_graceful_shutdown(([0, 0, 0, 0], port), async {
                shutdown_receiver.await.unwrap()
            })
            .1
            .await;
    };
}

fn get_key_and_cert(ip: IpAddr, cert_dir_path: &Path) -> (String, String) {
    if let Some(tuple) = find_key_and_cert(ip, cert_dir_path) {
        return tuple;
    }
    // No key/cert yet for that host. Generate self-signed.
    let (key, cert) = add_key_and_cert(ip);
    fs::create_dir_all(cert_dir_path).expect("couldn't create certificate directory");
    let (key_file_path, cert_file_path) = get_key_and_cert_paths(ip, cert_dir_path);
    fs::write(key_file_path, &key).expect("couldn't save key");
    fs::write(cert_file_path, &cert).expect("couldn't save certificate");
    (key, cert)
}

fn add_key_and_cert(ip: IpAddr) -> (String, String) {
    let mut params = CertificateParams::default();
    params.subject_alt_names = vec![SanType::IpAddress(ip)];
    let certificate = rcgen::Certificate::from_params(params)
        .expect("couldn't create self-signed server certificate");
    (
        certificate.serialize_private_key_pem(),
        certificate
            .serialize_pem()
            .expect("couldn't serialize self-signed server certificate"),
    )
}

fn find_key_and_cert(ip: IpAddr, cert_dir_path: &Path) -> Option<(String, String)> {
    let (key_file_path, cert_file_path) = get_key_and_cert_paths(ip, cert_dir_path);
    Some((
        fs::read_to_string(&key_file_path).ok()?,
        fs::read_to_string(&cert_file_path).ok()?,
    ))
}

fn get_key_and_cert_paths(ip: IpAddr, cert_dir_path: &Path) -> (PathBuf, PathBuf) {
    let ip_string = ip.to_string();
    let key_file_path = cert_dir_path.join(format!("{}.key", ip_string));
    let cert_file_path = cert_dir_path.join(format!("{}.pem", ip_string));
    (key_file_path, cert_file_path)
}

#[derive(Deserialize)]
struct WebSocketRequest {
    topics: String,
}

#[derive(Deserialize)]
struct PatchRequest {
    op: PatchRequestOp,
    path: String,
    value: serde_json::value::Value,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PatchRequestOp {
    Replace,
}

type Topics = HashSet<Topic>;

async fn client_connected(ws: WebSocket, topics: Topics, clients: ServerClients) {
    use futures::{FutureExt, StreamExt};
    use warp::Filter;
    let (ws_sender_sink, mut ws_receiver_stream) = ws.split();
    let (client_sender, client_receiver) = mpsc::unbounded_channel();
    // Keep forwarding received messages in client channel to websocket sender sink
    tokio::task::spawn(client_receiver.forward(ws_sender_sink).map(|result| {
        if let Err(e) = result {
            eprintln!("error sending websocket msg: {}", e);
        }
    }));
    let client_id = NEXT_CLIENT_ID.fetch_add(1, Ordering::Relaxed);
    let client = WebSocketClient {
        id: client_id,
        topics,
        sender: client_sender,
    };
    clients.write().unwrap().insert(client_id, client.clone());
    Reaper::get().do_later_in_main_thread_asap(move || {
        send_initial_events(&client);
    });
    // Keep receiving websocket receiver stream messages
    while let Some(result) = ws_receiver_stream.next().await {
        // We will need this as soon as we are interested in what the client says
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("websocket error: {}", e);
                break;
            }
        };
    }
    // Stream closed up, so remove from the client list
    clients.write().unwrap().remove(&client_id);
}

#[derive(Clone)]
pub struct WebSocketClient {
    id: usize,
    topics: Topics,
    sender: mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>,
}

impl WebSocketClient {
    fn send(&self, msg: impl Serialize) -> Result<(), &'static str> {
        let json = serde_json::to_string(&msg).map_err(|_| "couldn't serialize")?;
        self.sender
            .send(Ok(Message::text(json)))
            .map_err(|_| "couldn't send")
    }

    fn is_subscribed_to(&self, topic: &Topic) -> bool {
        self.topics.contains(topic)
    }
}

// We don't take the async RwLock by Tokio because we need to access this in sync code, too!
pub type ServerClients = Arc<std::sync::RwLock<HashMap<usize, WebSocketClient>>>;

pub fn keep_informing_clients_about_session_events(shared_session: &SharedSession) {
    let session = shared_session.borrow();
    when(
        session
            .on_mappings_changed()
            .merge(session.mapping_list_changed().map_to(()))
            .merge(session.mapping_changed().map_to(())),
    )
    .with(Rc::downgrade(shared_session))
    .do_async(|session, _| {
        let _ = send_updated_controller_routing(&session.borrow());
    });
    when(
        session
            .everything_changed()
            .merge(App::get().controller_manager().borrow().changed()),
    )
    .with(Rc::downgrade(shared_session))
    .do_async(|session, _| {
        let _ = send_updated_active_controller(&session.borrow());
        let _ = send_updated_controller_routing(&session.borrow());
    });
}

fn send_initial_controller_routing(
    client: &WebSocketClient,
    session_id: &str,
) -> Result<(), &'static str> {
    let session =
        session_manager::find_session_by_id(session_id).ok_or("couldn't find that session")?;
    let event = get_controller_routing_updated_event(&session.borrow());
    client.send(&event)
}

fn send_initial_controller(client: &WebSocketClient, session_id: &str) -> Result<(), &'static str> {
    let session =
        session_manager::find_session_by_id(session_id).ok_or("couldn't find that session")?;
    let event = get_active_controller_updated_event(&session.borrow());
    client.send(&event)
}

fn send_updated_active_controller(session: &Session) -> Result<(), &'static str> {
    send_to_clients_subscribed_to(
        &Topic::ActiveController {
            session_id: session.id().to_string(),
        },
        || get_active_controller_updated_event(session),
    )
}

fn send_updated_controller_routing(session: &Session) -> Result<(), &'static str> {
    send_to_clients_subscribed_to(
        &Topic::ControllerRouting {
            session_id: session.id().to_string(),
        },
        || get_controller_routing_updated_event(session),
    )
}

fn send_to_clients_subscribed_to<T: Serialize>(
    topic: &Topic,
    create_message: impl FnOnce() -> T,
) -> Result<(), &'static str> {
    let clients = App::get().server().borrow().clients()?.clone();
    let clients = clients
        .read()
        .map_err(|_| "couldn't get read lock for client")?;
    if clients.is_empty() {
        return Ok(());
    }
    let msg = create_message();
    for client in clients.values().filter(|c| c.is_subscribed_to(topic)) {
        let _ = client.send(&msg);
    }
    Ok(())
}

fn send_initial_events(client: &WebSocketClient) {
    for topic in &client.topics {
        let _ = send_initial_events_for_topic(client, topic);
    }
}

fn send_initial_events_for_topic(
    client: &WebSocketClient,
    topic: &Topic,
) -> Result<(), &'static str> {
    use Topic::*;
    match topic {
        ControllerRouting { session_id } => send_initial_controller_routing(client, session_id),
        ActiveController { session_id } => send_initial_controller(client, session_id),
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
enum Topic {
    ActiveController { session_id: String },
    ControllerRouting { session_id: String },
}

impl TryFrom<&str> for Topic {
    type Error = &'static str;

    fn try_from(topic_expression: &str) -> Result<Self, Self::Error> {
        let topic_segments: Vec<_> = topic_expression.split('/').skip(1).collect();
        let topic = match topic_segments.as_slice() {
            ["realearn", "session", id, "controller-routing"] => Topic::ControllerRouting {
                session_id: id.to_string(),
            },
            ["realearn", "session", id, "controller"] => Topic::ActiveController {
                session_id: id.to_string(),
            },
            _ => return Err("invalid topic expression"),
        };
        Ok(topic)
    }
}

fn get_active_controller_updated_event(session: &Session) -> Event<Option<ControllerData>> {
    Event::new(
        EventType::Updated,
        format!("/realearn/session/{}/controller", session.id()),
        get_controller(session),
    )
}

fn get_controller_routing_updated_event(session: &Session) -> Event<ControllerRouting> {
    Event::new(
        EventType::Updated,
        format!("/realearn/session/{}/controller-routing", session.id()),
        get_controller_routing(session),
    )
}

fn get_controller(session: &Session) -> Option<ControllerData> {
    let controller = session.active_controller()?;
    Some(ControllerData::from_model(&controller))
}

fn get_controller_routing(session: &Session) -> ControllerRouting {
    let routes = session
        .mappings(MappingCompartment::ControllerMappings)
        .filter_map(|m| {
            let m = m.borrow();
            let target_descriptor = if session.mapping_is_on(m.id()) {
                if m.target_model.category.get() == TargetCategory::Virtual {
                    let control_element = m.target_model.create_control_element();
                    let matching_primary_mappings: Vec<_> = session
                        .mappings(MappingCompartment::PrimaryMappings)
                        .filter(|mp| {
                            let mp = mp.borrow();
                            mp.source_model.category.get() == SourceCategory::Virtual
                                && mp.source_model.create_control_element() == control_element
                                && session.mapping_is_on(mp.id())
                        })
                        .collect();
                    if let Some(first_mapping) = matching_primary_mappings.first() {
                        let first_mapping = first_mapping.borrow();
                        let first_mapping_name = first_mapping.name.get_ref();
                        let label = if matching_primary_mappings.len() == 1 {
                            first_mapping_name.clone()
                        } else {
                            format!(
                                "{} +{}",
                                first_mapping_name,
                                matching_primary_mappings.len() - 1
                            )
                        };
                        TargetDescriptor { label }
                    } else {
                        return None;
                    }
                } else {
                    TargetDescriptor {
                        label: m.name.get_ref().clone(),
                    }
                }
            } else {
                return None;
            };
            Some((m.id().to_string(), target_descriptor))
        })
        .collect();
    ControllerRouting { routes }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ControllerRouting {
    routes: HashMap<String, TargetDescriptor>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TargetDescriptor {
    label: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Event<T> {
    r#type: EventType,
    path: String,
    payload: T,
}

impl<T> Event<T> {
    pub fn new(r#type: EventType, path: String, payload: T) -> Event<T> {
        Event {
            r#type,
            path,
            payload,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum EventType {
    Updated,
}

/// Inspired by local_ipaddress crate.
pub fn get_local_ip() -> Option<std::net::IpAddr> {
    let socket = match std::net::UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return None,
    };
    match socket.connect("8.8.8.8:80") {
        Ok(()) => (),
        Err(_) => return None,
    };
    match socket.local_addr() {
        Ok(addr) => return Some(addr.ip()),
        Err(_) => return None,
    };
}
