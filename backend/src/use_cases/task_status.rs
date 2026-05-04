use api_types::{TaskStatusInput, TaskStatusOutput};

use crate::providers::task_pool::TaskPool;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Task {0} not found")]
    TaskNotFound(uuid::Uuid),
}

impl From<Error> for crate::api::Error {
    fn from(value: Error) -> Self {
        crate::api::Error {
            code: "NotFound".to_string(),
            message: value.to_string(),
        }
    }
}

pub async fn command(
    task_pool: &TaskPool,
    input: TaskStatusInput,
) -> Result<TaskStatusOutput, Error> {
    let info = task_pool
        .get_status(input.task_id)
        .await
        .ok_or(Error::TaskNotFound(input.task_id))?;

    Ok(TaskStatusOutput {
        task_id: info.task_id,
        node_id: info.node_id,
        status: info.status,
        error_message: info.error_message,
    })
}
