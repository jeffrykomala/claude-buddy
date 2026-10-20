//! Slash commands typed into the bubble (design §8.6). A command is only
//! recognised when it is the whole message, so "/new project ideas" is an
//! ordinary prompt and Claude Code's own slash commands aren't hijacked.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommand {
    /// `/new`, `/clear`, `/reset`: start a fresh conversation.
    New,
    /// `/resume`: pick one of the recent conversations.
    Resume,
}

/// The command `message` is, if it is exactly one.
pub fn parse(message: &str) -> Option<SlashCommand> {
    match message.trim().to_lowercase().as_str() {
        "/new" | "/clear" | "/reset" => Some(SlashCommand::New),
        "/resume" => Some(SlashCommand::Resume),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_clear_and_reset_all_start_a_new_conversation() {
        for text in ["/new", "/clear", "/reset"] {
            assert_eq!(parse(text), Some(SlashCommand::New), "{text}");
        }
    }

    #[test]
    fn resume_opens_the_picker() {
        assert_eq!(parse("/resume"), Some(SlashCommand::Resume));
    }

    #[test]
    fn surrounding_whitespace_and_case_are_ignored() {
        assert_eq!(parse("  /NEW \n"), Some(SlashCommand::New));
    }

    #[test]
    fn a_command_with_more_text_is_a_normal_message() {
        assert_eq!(parse("/new project ideas"), None);
        assert_eq!(parse("please /new"), None);
    }

    #[test]
    fn other_text_and_unknown_commands_are_normal_messages() {
        assert_eq!(parse("hello"), None);
        assert_eq!(parse("/compact"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse("/"), None);
    }
}
