# 向用户展示的错误 / 失败消息字符串。
# Keys MUST start with `error-`.

error-unknown = 发生未知错误
error-retry = 重试
error-report = 报告问题

# 斜杠命令失败提示
error-no-active-conversation = 当前没有可导出的会话
error-session-already-shared = 该会话已被分享
error-slash-command-requires-ai = { $command } 需要先启用 AI
error-cannot-start-conversation-terminal-running = 终端命令运行中，无法开启新会话
error-rename-tab-missing-name = 请在 /rename-tab 后提供标签页名称
error-create-project-missing-description = 请在 /create-new-project 后描述要创建的项目
error-open-file-local-only = /open-file 命令仅在本地会话中可用
error-open-file-files-only = /open-file 命令只支持打开文件，不支持目录
error-open-file-not-found = 未找到文件：{ $path }
error-open-file-unsupported-build = 当前构建不支持 /open-file 命令
error-export-conversation-file-unsupported-web = 网页版不支持将会话导出为文件
error-conversation-cost-no-active-conversation = 无法显示会话费用：没有活动会话
error-conversation-cost-empty-conversation = 无法显示会话费用：会话为空
error-conversation-cost-in-progress = 无法显示会话费用：会话仍在进行中
error-fork-requires-active-conversation = /fork 需要一个活动会话
error-fork-and-compact-requires-active-conversation = /fork-and-compact 需要一个活动会话
error-compact-and-requires-active-conversation = /compact-and 需要一个活动会话
error-queue-requires-active-conversation = /queue 需要一个活动会话
error-queue-requires-prompt = /queue 需要一个提示词参数

# Agent 终端会话漂移防护
error-agent-terminal-session-lost = 智能体执行命令所在的终端会话已结束或切换（例如 SSH 连接断开）。命令不会被转到其他机器继续执行，该对话已停止。
agent-view-conversation-voided = 该对话已作废（智能体的 SSH 会话已断开），请开新对话继续。
agent-view-input-taken-over = 已接管输入：你键入的内容将发送到运行中的命令（回车发送）；再按 ⌘I 交还给智能体。
agent-view-input-handed-back = 输入已交还给智能体。
