use actix_web::web;

use crate::api::endpoints::{
    asset_thumbnail, asset_waveform, config_frontend, connect_nodes, disconnect_nodes, node_create,
    node_delete, node_file, node_position, node_thumbnail, project_get, projects_create,
    projects_delete, projects_list, projects_rename, run_node, task_status, upload_begin,
    upload_chunk, upload_finalize,
};

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.route(
        "/config/frontend.toml",
        web::get().to(config_frontend::handler),
    );
    cfg.service(
        web::scope("/api/projects")
            .route("/list", web::post().to(projects_list::handler))
            .route("/create", web::post().to(projects_create::handler))
            .route("/delete", web::post().to(projects_delete::handler))
            .route("/rename", web::post().to(projects_rename::handler))
            .route("/get", web::post().to(project_get::handler))
            .route(
                "/{project_id}/nodes/{node_type}/{node_id}/thumbnail",
                web::get().to(node_thumbnail::handler),
            )
            .route(
                "/{project_id}/nodes/{node_type}/{node_id}/file",
                web::get().to(node_file::handler),
            )
            .route(
                "/{project_id}/assets/{asset_id}/thumbnail",
                web::get().to(asset_thumbnail::handler),
            )
            .route(
                "/{project_id}/assets/{asset_id}/waveform",
                web::get().to(asset_waveform::handler),
            ),
    );
    cfg.service(
        web::scope("/api/nodes")
            .route("/create", web::post().to(node_create::handler))
            .route("/delete", web::post().to(node_delete::handler))
            .route("/position", web::post().to(node_position::handler))
            .route("/connect", web::post().to(connect_nodes::handler))
            .route("/disconnect", web::post().to(disconnect_nodes::handler))
            .route("/run", web::post().to(run_node::handler))
            .route("/task-status", web::post().to(task_status::handler)),
    );
    cfg.service(
        web::scope("/api/uploads")
            .app_data(web::PayloadConfig::new(8 * 1024 * 1024))
            .route("/begin", web::post().to(upload_begin::handler))
            .route("/chunk", web::post().to(upload_chunk::handler))
            .route("/finalize", web::post().to(upload_finalize::handler)),
    );
}
