mod structs;

use std::collections::BTreeMap;

use actix_cors::Cors;
#[cfg(debug_assertions)]
use actix_web::HttpResponse;
use actix_web::{App, HttpRequest, HttpServer, guard, middleware::Logger, web};
#[cfg(debug_assertions)]
use async_graphql::http::{GraphQLPlaygroundConfig, playground_source};
use async_graphql::{EmptySubscription, Schema};
use async_graphql_actix_web::{GraphQLRequest, GraphQLResponse};

use bson::doc;
use hmac::{Hmac, Mac};
use jwt::VerifyWithKey;
use sha2::Sha512;
use structs::{api::Database, meili::init_meili_index, schema::ObjectSchema, user::User};

use crate::structs::object::Object;

#[cfg(feature = "odc-ignore")]
lazy_static::lazy_static! {
    static ref ODC_IGNORE: Vec<String> = {
        let mut ignore: Vec<String> = vec![];

        match std::fs::File::open(".odcignore") {
            Ok(_) => {
                let content = std::fs::read_to_string(".odcignore").unwrap();
                log::info!("Found .odcignore file");

                for line in content.lines() {
                    if line.starts_with("#") || line.starts_with("//") || line.trim().len() == 0 { continue; }
                    ignore.push(line.trim().to_string());
                }
            },
            Err(_) => {
                log::info!("No .odcignore file found. Create one to ignore objects");
            }
        }

        log::info!("Ignoring objects: {:?}", ignore);
        ignore
    };
}

async fn get_user_from_token(token: &str, database: &Database) -> Option<User> {
    let private_key = std::env::var("JWT_PRIVATE_KEY").expect("JWT_PRIVATE_KEY must be set");
    let key: Hmac<Sha512> = Hmac::new_from_slice(private_key.as_bytes()).ok()?;

    let claims: BTreeMap<String, String> = token.verify_with_key(&key).ok()?;
    let subject = claims.get("sub")?;

    database
        .users
        .find_one(doc! {"uuid": subject}, None)
        .await
        .unwrap()
}

#[allow(clippy::future_not_send, reason = "We can't modify Actix")]
async fn graphql(
    schema: web::Data<ObjectSchema>,
    database: web::Data<Database>,
    req: HttpRequest,
    gql_request: GraphQLRequest,
) -> GraphQLResponse {
    let mut request = gql_request.into_inner();
    let database = database.into_inner();

    let token = req
        .headers()
        .get("Authorization")
        .and_then(|s| s.to_str().ok().map(String::from));
    drop(req);

    if let Some(token_str) = token
        && let Some(user) = get_user_from_token(&token_str, &database).await
    {
        request = request.data(user);
    }

    schema.execute(request).await.into()
}

#[cfg(debug_assertions)]
async fn graphql_playground() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(playground_source(GraphQLPlaygroundConfig::new("/")))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    let mongo_uri = std::env::var("MONGO_URI").expect("MONGO_URI must be set");
    let mongo_client = mongodb::Client::with_uri_str(mongo_uri).await.unwrap();
    let mongo_db = mongo_client
        .default_database()
        .expect("The MongoDB URI must have a database at the end.");

    let database = Database {
        objects: mongo_db.collection("odc"),
        collections: mongo_db.collection("odc_collections"),
        users: mongo_db.collection("odc_users"),
    };

    if let Err(err) = database.ensure_indexes().await {
        log::warn!("Failed to ensure MongoDB indexes: {}", err);
    }

    if let Ok(meili_url) = std::env::var("MEILI_URL") {
        init_meili_index(&meili_url, &database).await;
    }

    let mut schema = Schema::build(
        structs::schema::Query,
        structs::schema::Mutation,
        EmptySubscription,
    )
    .data(database.clone());

    // Disable introspection in production
    if !cfg!(debug_assertions) {
        schema = schema.disable_introspection();
    }

    let schema = schema.finish();

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .expect("PORT must be a number between 0 and 65535");

    // Check if `JWT_PRIVATE_KEY` is set
    if std::env::var("JWT_PRIVATE_KEY").is_err() {
        log::error!("JWT_PRIVATE_KEY must be set");
        std::process::exit(1);
    }

    // TODO: get CORS working

    HttpServer::new(move || {
        // let mut cors = Cors::default()
        //     .allowed_origin("https://web.destinationhome.live")
        //     .allowed_methods(vec!["GET", "POST", "OPTIONS", "HEAD"])
        //     .allow_any_header()
        //     .disable_vary_header()
        //     .max_age(3600);

        // #[cfg(debug_assertions)]
        // {
        let cors = Cors::default()
            .allow_any_header()
            .allow_any_method()
            .allow_any_origin()
            .send_wildcard()
            .max_age(3600);
        // }

        let app = App::new()
            .app_data(web::Data::new(schema.clone()))
            .app_data(web::Data::new(database.clone()))
            .service(web::resource("/graphql").guard(guard::Post()).to(graphql))
            .wrap(Logger::default())
            .wrap(cors);

        #[cfg(debug_assertions)]
        let app = app.service(
            web::resource("/")
                .guard(guard::Get())
                .to(graphql_playground),
        );

        app
    })
    .bind((host, port))?
    .run()
    .await
}
