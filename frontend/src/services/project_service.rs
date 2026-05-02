use api_types::{
    CreateNodeInput, CreateNodeOutput, CreateProjectInput, CreateProjectOutput,
    DeleteNodeInput, DeleteNodeOutput, DeleteProjectInput, DeleteProjectOutput,
    GetProjectInput, GetProjectOutput, ListProjectsOutput, NodeKind, Position,
    RenameProjectInput, RenameProjectOutput, UpdateNodePositionInput, UpdateNodePositionOutput,
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
