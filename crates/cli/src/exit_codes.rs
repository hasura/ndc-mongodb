#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitCode {
    CouldNotReadAggregationPipeline,
    CouldNotReadConfiguration,
    CouldNotProcessAggregationPipeline,
    ErrorWriting,
    InvalidArguments,
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
            ExitCode::InvalidArguments => 400,
            ExitCode::RefusedToOverwrite => 203,
            ExitCode::ResourceNotFound => 404,
        }
    }
}
