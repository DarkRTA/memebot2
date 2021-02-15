#![feature(proc_macro_hygiene, decl_macro)]
#[macro_use]
extern crate rocket;
use rusqlite::{params, Connection};
use rocket::config::{Config, Environment};
use rocket::{Request, Response};
use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::Header;
use std::error::Error;


pub struct CORS;

impl Fairing for CORS {
    fn info(&self) -> Info {
        Info {
            name: "Add CORS headers to requests",
            kind: Kind::Response
        }
    }

    fn on_response(&self, _request: &Request, response: &mut Response) {
        response.set_header(Header::new("Access-Control-Allow-Origin", "*"));
        response.set_header(Header::new("Access-Control-Allow-Methods", "POST, GET, PATCH, OPTIONS"));
        response.set_header(Header::new("Access-Control-Allow-Headers", "*"));
        response.set_header(Header::new("Access-Control-Allow-Credentials", "true"));
    }
}

struct Meme {
    id: i32,
    time: i64,
    text: String,
}

#[get("/<guild>")]
fn list(guild: u64) -> Result<String, Box<dyn Error>> {
    let conn = Connection::open("data.db")?; //FIXME: read config.rs
    let mut stmt = conn.prepare(&format!("SELECT * FROM x{}_memes", guild))?;
    let iter = stmt.query_map(params![], |row| {
        Ok(Meme {
            id: row.get(0)?,
            time: row.get(1).unwrap_or(0),
            text: row.get(2)?,
        })
    })?;

    Ok(iter
        .filter_map(|x| x.ok())
        .map(|x| format!("{} {} {}\n", x.id, x.time, x.text))
        .collect())
}
fn main() {
    let config = Config::build(Environment::Production)
        .address("127.0.0.1")
        .port(5360)
        .finalize().unwrap();
    rocket::custom(config).attach(CORS).mount("/", routes![list]).launch();
}
