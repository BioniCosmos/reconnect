use minijinja::{context, Environment, Value};
use minijinja_embed::load_templates;
use reqwest::{
    blocking::{Client, Response},
    Url,
};
use rocket::{fairing::AdHoc, get, launch, response::content::RawHtml, routes, tokio::task, State};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::HashMap,
    error::Error,
    sync::{
        mpsc::{self, Receiver},
        Mutex, RwLock,
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

type Receivers = RwLock<HashMap<String, Mutex<Receiver<String>>>>;

const BASE_URL: &str = "http://192.168.0.1/";

#[launch]
fn rocket() -> _ {
    let mut env = Environment::new();
    load_templates!(&mut env);
    rocket::build()
        .mount("/", routes![index, reconnect, echo])
        .attach(AdHoc::config::<Config>())
        .manage(Receivers::new(HashMap::new()))
        .manage(env)
}

#[get("/")]
async fn index(
    config: &State<Config>,
    env: &State<Environment<'_>>,
) -> Result<RawHtml<String>, String> {
    let password = config.password.clone();
    let ip = task::spawn_blocking(move || {
        let client = Client::new();
        login(&client, &password).and_then(|stok| get_ip(&client, &stok))
    })
    .await
    .map_err_to_string()??;
    render(env, "index.html.j2", context! { ip })
}

#[get("/api/reconnect")]
fn reconnect(config: &State<Config>, receivers: &State<Receivers>) -> String {
    let password = config.password.clone();
    let (tx, rx) = mpsc::channel();
    let id = Uuid::new_v4().to_string();
    receivers
        .write()
        .unwrap()
        .insert(id.clone(), Mutex::new(rx));
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
    let receivers = receivers.read().unwrap();
    receivers
        .get(id)
        .map(|rx| rx.lock().unwrap().recv().unwrap())
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

fn render(env: &Environment, name: &str, ctx: Value) -> Result<RawHtml<String>, String> {
    env.get_template(name)
        .and_then(|tmpl| tmpl.render(ctx))
        .map(RawHtml)
        .map_err_to_string()
}
