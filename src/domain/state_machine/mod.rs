mod survey_input_state_state_machine;
mod survey_session_state_state_machine;

/// Shared error type for all state machines in this module
#[derive(Debug, Clone, thiserror::Error)]
pub enum StateMachineError {
    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Invalid transition: {0}")]
    InvalidTransition(String),

    #[error("Transition '{transition}' not allowed from state '{from}'")]
    TransitionNotAllowed {
        transition: String,
        from: String,
    },

    #[error("Role '{role}' not authorized for transition '{transition}'")]
    RoleNotAuthorized {
        role: String,
        transition: String,
    },

    #[error("Guard condition failed for transition '{0}'")]
    GuardFailed(String),

    #[error("Cannot transition from final state '{0}'")]
    FinalStateReached(String),
}

pub use survey_input_state_state_machine::{survey_input_stateState, survey_input_stateTransition, survey_input_stateStateMachine};
pub use survey_session_state_state_machine::{survey_session_stateState, survey_session_stateTransition, survey_session_stateStateMachine};
