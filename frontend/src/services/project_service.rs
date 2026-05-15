use api_types::{NodeSettings, UpdateNodeSettingsInput, UpdateNodeSettingsOutput,
    ConnectNodesInput, ConnectNodesOutput, CreateNodeInput, CreateNodeOutput,
    CreateProjectInput, CreateProjectOutput, DeleteNodeInput, DeleteNodeOutput,
    DeleteProjectInput, DeleteProjectOutput, DisconnectNodesInput, DisconnectNodesOutput,
    GetProjectInput, GetProjectOutput, ListProjectsOutput, NodeKind, Position,
    RenameProjectInput, RenameProjectOutput, RunNodeInput, RunNodeOutput, TaskStatusInput,
    TaskStatusOutput, UpdateNodePositionInput, UpdateNodePositionOutput,
};
use uuid::Uuid;

use crate::services::api::{post, ApiClientError};

pub async fn list_projects() -> Result<ListProjectsOutput, ApiClientError> {
    post::<(), ListProjectsOutput>("/api/projects/list", &()).await
}

pub async fn create_project(name: String) -> Result<CreateProjectOutput, ApiClientError> {
    post("/api/projects/create", &CreateProjectInput { name }).await
}

pub async fn delete_project(id: Uuid) -> Result<DeleteProjectOutput, ApiClientError> {
    post("/api/projects/delete", &DeleteProjectInput { id }).await
}

pub async fn rename_project(
    id: Uuid,
    new_name: String,
) -> Result<RenameProjectOutput, ApiClientError> {
    post(
        "/api/projects/rename",
        &RenameProjectInput { id, new_name },
    )
    .await
}

pub async fn get_project(id: Uuid) -> Result<GetProjectOutput, ApiClientError> {
    post("/api/projects/get", &GetProjectInput { id }).await
}

pub async fn create_node(
    project_id: Uuid,
    kind: NodeKind,
    position: Position,
    parent_map_id: Option<Uuid>,
) -> Result<CreateNodeOutput, ApiClientError> {
    post(
        "/api/nodes/create",
        &CreateNodeInput {
            project_id,
            kind,
            position,
            parent_map_id,
        },
    )
    .await
}

pub async fn delete_node(
    project_id: Uuid,
    node_id: Uuid,
    parent_map_id: Option<Uuid>,
) -> Result<DeleteNodeOutput, ApiClientError> {
    post(
        "/api/nodes/delete",
        &DeleteNodeInput {
            project_id,
            node_id,
            parent_map_id,
        },
    )
    .await
}

pub async fn update_node_position(
    project_id: Uuid,
    node_id: Uuid,
    position: Position,
) -> Result<UpdateNodePositionOutput, ApiClientError> {
    post(
        "/api/nodes/position",
        &UpdateNodePositionInput {
            project_id,
            node_id,
            position,
            parent_map_id: None, // TODO: pass from editor
        },
    )
    .await
}

pub async fn connect_nodes(
    project_id: Uuid,
    from_node: Uuid,
    from_port: String,
    to_node: Uuid,
    to_port: String,
    parent_map_id: Option<Uuid>,
) -> Result<ConnectNodesOutput, ApiClientError> {
    post(
        "/api/nodes/connect",
        &ConnectNodesInput {
            project_id,
            from_node,
            from_port,
            to_node,
            to_port,
            parent_map_id,
        },
    )
    .await
}

pub async fn disconnect_nodes(
    project_id: Uuid,
    from_node: Uuid,
    from_port: String,
    to_node: Uuid,
    to_port: String,
    parent_map_id: Option<Uuid>,
) -> Result<DisconnectNodesOutput, ApiClientError> {
    post(
        "/api/nodes/disconnect",
        &DisconnectNodesInput {
            project_id,
            from_node,
            from_port,
            to_node,
            to_port,
            parent_map_id,
        },
    )
    .await
}

pub async fn invalidate_node(
    project_id: Uuid,
    node_id: Uuid,
) -> Result<RunNodeOutput, ApiClientError> {
    post(
        "/api/nodes/invalidate",
        &RunNodeInput {
            project_id,
            node_id,
        },
    )
    .await
}

pub async fn run_node(
    project_id: Uuid,
    node_id: Uuid,
) -> Result<RunNodeOutput, ApiClientError> {
    post(
        "/api/nodes/run",
        &RunNodeInput {
            project_id,
            node_id,
        },
    )
    .await
}

pub async fn update_node_settings(
    project_id: Uuid,
    node_id: Uuid,
    settings: NodeSettings,
) -> Result<UpdateNodeSettingsOutput, ApiClientError> {
    post(
        "/api/nodes/settings",
        &UpdateNodeSettingsInput {
            project_id,
            node_id,
            settings,
        },
    )
    .await
}

pub async fn get_task_status(task_id: Uuid) -> Result<TaskStatusOutput, ApiClientError> {
    post("/api/nodes/task-status", &TaskStatusInput { task_id }).await
}

pub async fn save_template(
    project_id: Uuid,
    name: String,
    node_ids: Vec<Uuid>,
    parent_map_id: Option<Uuid>,
) -> Result<api_types::SaveTemplateOutput, ApiClientError> {
    post(
        "/api/templates/save",
        &api_types::SaveTemplateInput { project_id, name, node_ids, parent_map_id },
    )
    .await
}

pub async fn list_templates() -> Result<api_types::ListTemplatesOutput, ApiClientError> {
    post("/api/templates/list", &()).await
}

pub async fn unpack_template(
    project_id: Uuid,
    template_name: String,
    position: api_types::Position,
    parent_map_id: Option<Uuid>,
) -> Result<api_types::UnpackTemplateOutput, ApiClientError> {
    post(
        "/api/templates/unpack",
        &api_types::UnpackTemplateInput { project_id, template_name, position, parent_map_id },
    )
    .await
}

pub async fn delete_template(name: String) -> Result<(), ApiClientError> {
    post::<_, ()>("/api/templates/delete", &api_types::DeleteTemplateInput { name }).await
}
