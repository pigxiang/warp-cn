use crate::search::mixer::SearchMixer;
pub type CommandSearchMixer = SearchMixer<CommandSearchItemAction>;

#[derive(Clone, Debug)]
pub struct AcceptedHistoryItem {
    pub command: String,
}

/// The set of events that may be produced by accepting or executing a search
/// result.
#[derive(Clone, Debug)]
pub enum CommandSearchItemAction {
    /// The user accepted a history search item. The contained string is the
    /// command they accepted.
    AcceptHistory(AcceptedHistoryItem),

    /// The user requested the re-execution of a history search item. The
    /// contained string is the command they accepted.
    ExecuteHistory(String),

    /// The user accepted the AI query search item with this query text.
    AcceptAIQuery(String),

    /// The user requested to run the AI query search item with this query text.
    RunAIQuery(String),
}

#[cfg(test)]
#[path = "searcher_tests.rs"]
mod tests;
