#[macro_use] extern crate rocket;
#[macro_use] extern crate rocket_sync_db_pools;

use rocket::{Rocket, Build};
use rocket::fairing::AdHoc;
use rocket::serde::{Serialize, Deserialize, json::Json};
use rocket::response::{Debug, status::Created};

use rocket_sync_db_pools::{rusqlite};

use self::rusqlite::params;

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

#[post("/location", data = "<data>")] 
async fn create(db: Db, data: Json<Loc>)  -> Result<Created<Json<Loc>>> {
    let item = data.clone();
    db.run(move |conn| {
        conn.execute("REPLACE INTO locs (plate, latitude, longitude, speed) VALUES (?1, ?2, ?3, ?4)", 
        params![ item.plate, item.latitude, item.longitude, item.speed])
    }).await?;

    Ok(Created::new("/").body(data))
}


#[get("/")]
async fn list(db: Db) -> Result<Json<Vec<String>>> {
    let ids = db.run(|conn| {
        conn.prepare("SELECT plate FROM locs")?
            .query_map(params![], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()
    }).await?;

    Ok(Json(ids))
}

#[get("/<plate>")]
async fn read(db: Db, plate: String) -> Option<Json<Loc>> {
    let loc = db.run(move |conn| {
        conn.query_row("SELECT * FROM locs WHERE plate = ?1", params![plate],
            |r| Ok(Loc {plate: r.get(0)?, latitude: r.get(1)?, longitude: r.get(2)?, speed: r.get(3)? }))
    }).await.ok()?;

    Some(Json(loc))
}

#[delete("/<plate>")]
async fn delete(db: Db, plate: String) -> Result<Option<()>> {
    let affected = db.run(move |conn| {
        conn.execute("DELETE FROM locs WHERE plate = ?1", params![plate])
    }).await?;

    Ok((affected == 1).then(|| ()))
}

#[delete("/")]
async fn destroy(db: Db) -> Result<()> {
    db.run(move |conn| conn.execute("DELETE FROM locs", params![])).await?;

    Ok(())
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
            .mount("/", routes![list, create, read, delete, destroy])
    })
}



#[launch]
fn rocket_run() -> _ {
    rocket::build()
        .attach(stage())
}
