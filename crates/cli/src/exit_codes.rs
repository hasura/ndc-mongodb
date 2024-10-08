#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitCode {
    CouldNotReadAggregationPipeline,
    CouldNotReadConfiguration,
    ErrorWriting,
    RefusedToOverwrite,
}

impl From<ExitCode> for i32 {
    fn from(value: ExitCode) -> Self {
        match value {
            ExitCode::CouldNotReadAggregationPipeline => 201,
            ExitCode::CouldNotReadConfiguration => 202,
            ExitCode::ErrorWriting => 204,
            ExitCode::RefusedToOverwrite => 203,
        }
    }
}
