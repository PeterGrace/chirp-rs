// Memory validation logic and helpers

use super::features::{RadioFeatures, ValidationMessage};
use super::memory::Memory;

/// Validate a memory against radio features and return messages
pub fn validate_memory(features: &RadioFeatures, memory: &Memory) -> Vec<ValidationMessage> {
    features.validate_memory(memory)
}

/// Check if validation messages contain any errors
pub fn has_errors(messages: &[ValidationMessage]) -> bool {
    messages.iter().any(|m| m.is_error())
}

/// Check if validation messages contain any warnings
pub fn has_warnings(messages: &[ValidationMessage]) -> bool {
    messages.iter().any(|m| m.is_warning())
}

/// Filter out only error messages
pub fn errors_only(messages: &[ValidationMessage]) -> Vec<String> {
    messages
        .iter()
        .filter(|m| m.is_error())
        .map(|m| m.message().to_string())
        .collect()
}

/// Filter out only warning messages
pub fn warnings_only(messages: &[ValidationMessage]) -> Vec<String> {
    messages
        .iter()
        .filter(|m| m.is_warning())
        .map(|m| m.message().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_helpers() {
        let msgs = vec![
            ValidationMessage::Warning("test warning".to_string()),
            ValidationMessage::Error("test error".to_string()),
        ];

        assert!(has_errors(&msgs));
        assert!(has_warnings(&msgs));

        let errors = errors_only(&msgs);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0], "test error");

        let warnings = warnings_only(&msgs);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0], "test warning");
    }
}
