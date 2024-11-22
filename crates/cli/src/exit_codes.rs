#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitCode {
    CouldNotReadAggregationPipeline,
    CouldNotReadConfiguration,
    CouldNotProcessAggregationPipeline,
    ErrorWriting,
    RefusedToOverwrite,
    ResourceNotFound,
}

impl From<ExitCode> for i32 {
    fn from(value: ExitCode) -> Self {
        match value {
            ExitCode::CouldNotReadAggregationPipeline => 201,
            ExitCode::CouldNotReadConfiguration => 202,
            ExitCode::CouldNotProcessAggregationPipeline => 205,
            ExitCode::ErrorWriting => 204,
            ExitCode::RefusedToOverwrite => 203,
            ExitCode::ResourceNotFound => 206,
        }
    }
}
