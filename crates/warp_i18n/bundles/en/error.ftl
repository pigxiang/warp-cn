# Error / failure message strings shown to users.
# Keys MUST start with `error-`.

error-unknown = An unknown error occurred
error-retry = Retry
error-report = Report issue

# Slash command failures
error-no-active-conversation = No active conversation to export
error-session-already-shared = Session is already being shared
error-slash-command-requires-ai = { $command } requires AI to be enabled
error-cannot-start-conversation-terminal-running = cannot start new conversation while terminal command is running
error-rename-tab-missing-name = Please provide a tab name after /rename-tab
error-create-project-missing-description = Please describe the project you want to create after /create-new-project
error-open-file-local-only = The /open-file command is only available for local sessions
error-open-file-files-only = The /open-file command only works for files, not directories
error-open-file-not-found = File not found: { $path }
error-open-file-unsupported-build = The /open-file command is not supported in this build
error-export-conversation-file-unsupported-web = Export conversation to file unsupported in web
error-conversation-cost-no-active-conversation = Cannot show conversation cost: no active conversation
error-conversation-cost-empty-conversation = Cannot show conversation cost: conversation is empty
error-conversation-cost-in-progress = Cannot show conversation cost: conversation is in progress
error-fork-requires-active-conversation = /fork requires an active conversation
error-fork-and-compact-requires-active-conversation = /fork-and-compact requires an active conversation
error-compact-and-requires-active-conversation = /compact-and requires an active conversation
error-queue-requires-active-conversation = /queue requires an active conversation
error-queue-requires-prompt = /queue requires a prompt argument

# Agent terminal-session drift guard
error-agent-terminal-session-lost = The terminal session the agent was running commands in has ended or switched (e.g. an SSH connection closed). Commands were not re-targeted to another machine, and the conversation was stopped.
agent-view-conversation-voided = This conversation has been voided (the agent's SSH session ended). Start a new conversation to continue.
agent-view-input-taken-over = Input taken over: typed text now goes to the running command (Enter sends it). Press Cmd+I again to hand it back to the agent.
agent-view-input-handed-back = Input handed back to the agent.
