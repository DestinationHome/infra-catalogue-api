mod structs;

use actix_cors::Cors;
use actix_web::{web, App, HttpServer, guard, HttpResponse};
use async_graphql::{Schema, EmptyMutation, EmptySubscription, http::{GraphQLPlaygroundConfig, playground_source}};
use async_graphql_actix_web::{GraphQLRequest, GraphQLResponse};

use structs::{
    api::Database,
    schema::ObjectSchema
};

use crate::structs::object::Object;

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

async fn index(schema: web::Data<ObjectSchema>, req: GraphQLRequest) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
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
    let mongo_db = mongo_client.default_database().expect("The MongoDB URI must have a database at the end.");
    let mongo_collection = mongo_db.collection("odc");

    let database = Database {
        client: mongo_client,
        database: mongo_db,
        collection: mongo_collection,
    };

    let schema = Schema::build(
        structs::schema::Query,
        EmptyMutation,
        EmptySubscription
    )
    .disable_introspection()
    .data(database.clone())
    .finish();

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string()).parse::<u16>().expect("PORT must be a number between 0 and 65535");

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

        let mut app = App::new()
            .app_data(web::Data::new(schema.clone()))
            .service(web::resource("/").guard(guard::Post()).to(index))
            .wrap(cors);

        #[cfg(debug_assertions)]
        {
            app = app.service(
                web::resource("/")
                    .guard(guard::Get())
                    .to(graphql_playground),
            )
        }

        app
        }
    )
        .bind((host, port))?
        .run()
        .await
}
