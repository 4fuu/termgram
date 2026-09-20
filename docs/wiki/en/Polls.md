# Polls and quizzes

[简体中文](../zh-CN/Polls.md) · [Guide](Home.md)

Poll questions and answers appear inline with message text. Select a poll and
press `v` or run `:poll` to open its voting panel. Enter also opens the panel when
the selected message action is the poll. The panel first refreshes the message;
cached polls remain readable while offline.

| In the poll panel | Action |
| --- | --- |
| j/k or Down/Up | Move between answers |
| Click an answer, Space or Enter | Toggle the selected answer locally |
| Ctrl-S | Submit the selected answers |
| u, then Ctrl-S | Retract an existing vote, when Telegram permits it |
| Ctrl-R | Refresh the poll and restore the server's current selection |
| PageUp/PageDown, Home/End, mouse wheel | Scroll long questions, answers or errors |
| s | Reveal or hide spoilers |
| Esc or q | Close the panel |

Choosing an answer does not send a vote. A single-choice poll keeps one selected
answer; multiple-choice polls allow several. Empty selection retracts an existing
vote only after Ctrl-S. Quiz answers and polls that prohibit revoting cannot be
changed. Closing the panel discards an unsent selection; a submitted request
continues in the background. If the request fails or its result is uncertain,
refresh before retrying. Refresh replaces the local selection with the server's
confirmed choices.

The panel identifies public votes versus anonymous votes before submission.
Telegram checks membership, country restrictions and other eligibility rules.
Changed options invalidate an open selection. The request checks the latest
message again and sends Telegram's stable answer IDs, so answer reordering cannot
redirect a vote to a different answer.

Vote counts and percentages appear after voting or closing, subject to Telegram's
“hide results until close” setting. Quiz explanations appear when results are
available. Spoilers remain masked; the first selection on a concealed poll
reveals its text without choosing an answer. Poll state also appears in the
bottom bar's `message` component. Contextual keys use your effective Lua bindings.

Live updates refresh cached copies of the same poll. Visible polls additionally
refresh in the background, normally every 30 seconds, with at most two requests
at once. Leaving the view or losing terminal focus stops new refreshes. Completed
closed polls stop periodic refreshes. Cached vote counts may be older while
offline; opening the voting panel always requests fresh information.

This revision reads ordinary polls and quizzes, submits votes and retracts votes.
Poll creation, voter lists, adding answers to an open-answer poll and media inside
poll questions or answers are not implemented. Media polls show an explicit
read-only notice to review and vote in an official client.

Rebind entry with `run = "poll"` in `conversation`. The `poll` context supports
`up`, `down`, `toggle_poll_answer`, `send`, `retract_vote`, `refresh`, `spoilers`,
`page_up`, `page_down`, `home`, `end`, and `cancel`.
