use api_types::{
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
) -> Result<CreateNodeOutput, ApiClientError> {
    post(
        "/api/nodes/create",
        &CreateNodeInput {
            project_id,
            kind,
            position,
        },
    )
    .await
}

pub async fn delete_node(
    project_id: Uuid,
    node_id: Uuid,
) -> Result<DeleteNodeOutput, ApiClientError> {
    post(
        "/api/nodes/delete",
        &DeleteNodeInput {
            project_id,
            node_id,
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
) -> Result<ConnectNodesOutput, ApiClientError> {
    post(
        "/api/nodes/connect",
        &ConnectNodesInput {
            project_id,
            from_node,
            from_port,
            to_node,
            to_port,
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
) -> Result<DisconnectNodesOutput, ApiClientError> {
    post(
        "/api/nodes/disconnect",
        &DisconnectNodesInput {
            project_id,
            from_node,
            from_port,
            to_node,
            to_port,
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

pub async fn get_task_status(task_id: Uuid) -> Result<TaskStatusOutput, ApiClientError> {
    post("/api/nodes/task-status", &TaskStatusInput { task_id }).await
}
