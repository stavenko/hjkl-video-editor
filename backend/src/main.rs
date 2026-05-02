use std::path::PathBuf;
use std::sync::Arc;

use actix_cors::Cors;
use actix_web::web;
use clap::Parser;

use backend::api;
use backend::api::endpoints::config_frontend::FrontendConfigPath;
use backend::config::Config;
use backend::providers::{Ffmpeg, ProjectStorage, UploadManager};

#[derive(Parser, Debug)]
#[command(name = "backend")]
#[command(about = "hjkl-video-editor backend")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    Run {
        #[arg(long)]
        config: PathBuf,
    },
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run { config } => run_server(config).await,
    }
}

async fn run_server(config_path: PathBuf) -> std::io::Result<()> {
    let config = Config::from_file(&config_path).expect("Failed to load configuration file");

    let project_storage = Arc::new(
        ProjectStorage::new(config.storage.projects_root.clone())
            .await
            .expect("Failed to initialize project storage"),
    );

    let frontend_config_path = FrontendConfigPath(config.frontend.config_path.clone());
    let frontend_config_path_data = web::Data::new(frontend_config_path);
    let upload_manager_data = web::Data::new(UploadManager::new());
    let ffmpeg_data = web::Data::new(Ffmpeg::new(config.ffmpeg.binary.clone()));

    let bind_addr = config.addr.clone();
    let bind_port = config.port;

    let server = actix_web::HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        actix_web::App::new()
            .wrap(cors)
            .app_data(web::Data::new(project_storage.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data(frontend_config_path_data.clone())
            .app_data(upload_manager_data.clone())
            .app_data(ffmpeg_data.clone())
            .configure(api::configure_routes)
    })
    .bind((bind_addr.as_str(), bind_port))?
    .run();

    tracing::info!("Backend starting on http://{}:{}", bind_addr, bind_port);
    server.await
}
