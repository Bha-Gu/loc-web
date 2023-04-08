#[macro_use] extern crate rocket;
#[macro_use] extern crate rocket_sync_db_pools;

use std::collections::HashSet;

use rocket::{Rocket, Build};
use rocket::fairing::AdHoc;
use rocket::serde::{Serialize, Deserialize, json::Json};
use rocket::response::{Debug, status::Created};

use rocket_sync_db_pools::{rusqlite};

use self::rusqlite::params;

use rocket::error::Error;
use rocket::http::{self};
use rocket::response::Responder;
use rocket::{get, options, routes, State};
use rocket_cors::{AllowedHeaders, AllowedOrigins, Cors, CorsOptions, Method, Guard};

#[database("rusqlite")]
struct Db(rusqlite::Connection);


type Result<T, E = Debug<rusqlite::Error>> = std::result::Result<T, E>;


#[derive(Debug, Clone, FromForm, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct Loc {
    latitude: f64,
    longitude: f64,
    speed: f32,
    plate: String,

}


fn core_options() -> CorsOptions {
    rocket_cors::CorsOptions {
        allowed_origins: AllowedOrigins::all(),
        allowed_methods: vec![http::Method::Get, http::Method::Post, http::Method::Options].into_iter().map(|method| Method(method)).collect(),
        allowed_headers: AllowedHeaders::all(),
        allow_credentials: true,
        fairing_route_base: "/".to_owned(),
        max_age: Some(42),
        ..Default::default()
    }
}

#[post("/location", data = "<data>")] 
async fn create<'r, 'o: 'r>(db: Db, data: Json<Loc>)  -> Result<impl Responder<'r, 'o>> {
    let item = data.clone();
    db.run(move |conn| {
        conn.execute("REPLACE INTO locs (plate, latitude, longitude, speed) VALUES (?1, ?2, ?3, ?4)", 
        params![ item.plate, item.latitude, item.longitude, item.speed])
    }).await?;

    let options = match core_options().to_cors() {
        Ok(a) => a,
        Err(a) => return Ok(Err(a))
    };
    Ok(options.respond_owned(|guard| guard.responder(data)))
}

#[options("/location")]
async fn create_options<'r, 'o: 'r>() -> impl Responder<'r, 'o> {
    let options = core_options().to_cors()?;
    options.respond_owned(|guard| guard.responder(()))
}

#[get("/")]
async fn list<'r, 'o: 'r>(db: Db) -> Result<impl Responder<'r, 'o>> {
    let ids = db.run(|conn| {
        conn.prepare("SELECT plate FROM locs")?
            .query_map(params![], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()
    }).await?;

    
    let options = match core_options().to_cors() {
        Ok(a) => a,
        Err(a) => return Ok(Err(a))
    };
    Ok(options.respond_owned(|guard| guard.responder(Json(ids))))
}

#[options("/")]
async fn list_options<'r, 'o: 'r>() -> impl Responder<'r, 'o> {
    let options = core_options().to_cors()?;
    options.respond_owned(|guard| guard.responder(()))
}

#[get("/<plate>")]
async fn read<'r, 'o: 'r>(db: Db, plate: String) -> Result<impl Responder<'r, 'o>> {
    let loc = db.run(move |conn| {
        conn.query_row("SELECT * FROM locs WHERE plate = ?1", params![plate],
            |r| Ok(Loc {plate: r.get(0)?, latitude: r.get(1)?, longitude: r.get(2)?, speed: r.get(3)? }))
    }).await?;

    let options = match core_options().to_cors() {
        Ok(a) => a,
        Err(a) => return Ok(Err(a))
    };
    Ok(options.respond_owned(|guard| guard.responder(Json(loc))))
}

#[delete("/<plate>")]
async fn delete<'r, 'o: 'r>(db: Db, plate: String) -> Result<impl Responder<'r, 'o>> {
    let affected = db.run(move |conn| {
        conn.execute("DELETE FROM locs WHERE plate = ?1", params![plate])
    }).await?;

    let out = (affected == 1).then(|| ());
    let options = match core_options().to_cors() {
        Ok(a) => a,
        Err(a) => return Ok(Err(a))
    };
    Ok(options.respond_owned(move |guard| guard.responder(out)))
}

#[delete("/")]
async fn destroy<'r, 'o: 'r>(db: Db) -> Result<impl Responder<'r, 'o>> {
    db.run(move |conn| conn.execute("DELETE FROM locs", params![])).await?;

    let options = match core_options().to_cors() {
        Ok(a) => a,
        Err(a) => return Ok(Err(a))
    };
    Ok(options.respond_owned(|guard| guard.responder(())))
}

async fn init_db(rocket: Rocket<Build>) -> Rocket<Build> {
    Db::get_one(&rocket).await
        .expect("database mounted")
        .run(|conn| {
            conn.execute(r#"
                CREATE TABLE locs (
                    plate VARCHAR NOT NULL PRIMARY KEY,
                    latitude FLOAT NOT NULL,
                    longitude FLOAT NOT NULL,
                    speed FLOAT NOT NULL,
                    published BOOLEAN NOT NULL DEFAULT 0
                )"#, params![])
        }).await
        .expect("can init rusqlite DB");

    rocket
}

fn stage() -> AdHoc {
    AdHoc::on_ignite("Rusqlite Stage", |rocket| async {
        rocket.attach(Db::fairing())
            .attach(AdHoc::on_ignite("Rusqlite Init", init_db))
            .manage(core_options().to_cors().expect("Not to fail"))
            .mount("/", routes![list, create, read, delete, destroy, create_options, list_options])
            .mount("/", rocket_cors::catch_all_options_routes())
            
    })
}



#[launch]
fn rocket_run() -> _ {
    rocket::build()
        .attach(stage())
}
