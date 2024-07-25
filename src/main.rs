use reqwest::{
    blocking::{Client, Response},
    Url,
};
use rocket::{fairing::AdHoc, get, launch, routes, tokio::task, State};
use rocket_dyn_templates::{context, Template};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::HashMap,
    error::Error,
    sync::{
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use uuid::Uuid;

#[derive(Deserialize)]
struct Config {
    password: String,
}

#[derive(Deserialize)]
struct LoginResponse {
    stok: String,
}

#[derive(Deserialize)]
struct IPResponse {
    network: Network,
}

#[derive(Deserialize)]
struct Network {
    wan_status: WanStatus,
}

#[derive(Deserialize)]
struct WanStatus {
    ipaddr: String,
}

type Receivers = Arc<Mutex<HashMap<String, Receiver<String>>>>;

const BASE_URL: &str = "http://192.168.0.1/";

#[launch]
fn rocket() -> _ {
    rocket::build()
        .mount("/", routes![index, reconnect, echo])
        .attach(Template::fairing())
        .attach(AdHoc::config::<Config>())
        .manage(Receivers::new(Mutex::new(HashMap::new())))
}

#[get("/")]
async fn index(config: &State<Config>) -> Result<Template, String> {
    let password = config.password.clone();
    let ip = task::spawn_blocking(move || {
        let client = Client::new();
        login(&client, &password).and_then(|stok| get_ip(&client, &stok))
    })
    .await
    .map_err_to_string()??;
    Ok(Template::render("index", context! { ip }))
}

#[get("/api/reconnect")]
fn reconnect(config: &State<Config>, receivers: &State<Receivers>) -> String {
    let password = config.password.clone();
    let (tx, rx) = mpsc::channel();
    let id = Uuid::new_v4().to_string();
    receivers.lock().unwrap().insert(id.clone(), rx);
    task::spawn_blocking(move || {
        thread::sleep(Duration::from_secs(1));
        let client = Client::new();
        let message = login(&client, &password)
            .and_then(|stok| switch_wan(&client, &stok, "disconnect").and(Ok(stok)))
            .inspect(|_| thread::sleep(Duration::from_secs(5)))
            .and_then(|stok| switch_wan(&client, &stok, "connect"))
            .and(Ok("Done".to_string()));
        tx.send(message.unwrap_or_else(|e| e)).unwrap();
    });
    id
}

#[get("/api/echo/<id>")]
fn echo(id: &str, receivers: &State<Receivers>) -> Option<String> {
    let receivers = receivers.lock().unwrap();
    receivers.get(id).map(|rx| rx.recv().unwrap())
}

fn login(client: &Client, password: &str) -> Result<String, String> {
    client
        .post(BASE_URL)
        .json(&json!({
            "method": "do",
            "login": { "password": password }
        }))
        .send()
        .and_then(Response::error_for_status)
        .map(Response::json::<LoginResponse>)
        .map(Result::unwrap)
        .map(|res| res.stok)
        .map_err_to_string()
}

fn switch_wan(client: &Client, stok: &str, operation: &str) -> Result<(), String> {
    client
        .post(get_url(stok))
        .json(&json!({
            "method": "do",
            "network": {
                "change_wan_status": {
                    "proto": "pppoe",
                    "operate": operation
                }
            }
        }))
        .send()
        .and_then(Response::error_for_status)
        .map(|_| ())
        .map_err_to_string()
}

fn get_ip(client: &Client, stok: &str) -> Result<String, String> {
    client
        .post(get_url(stok))
        .json(&json!({
            "network": { "name": ["wan_status"] },
            "method": "get"
        }))
        .send()
        .and_then(Response::error_for_status)
        .map(Response::json::<IPResponse>)
        .map(Result::unwrap)
        .map(|res| res.network.wan_status.ipaddr)
        .map_err_to_string()
}

fn get_url(stok: &str) -> Url {
    Url::parse(BASE_URL)
        .unwrap()
        .join(&format!("/stok={stok}/ds"))
        .unwrap()
}

trait MapErrorToString<T> {
    fn map_err_to_string(self) -> Result<T, String>;
}

impl<T, E: Error> MapErrorToString<T> for Result<T, E> {
    fn map_err_to_string(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}
