A Discord bot for receiving, reviewing and compiling anonymous confessions for
a high school.

## Setup

You need a public domain to host the bot on. If you don't have a static IP, `cloudflared`
is your friend.

Copy `.env.example` to `.env` and fill in the variables
- The `DISCORD_` variables can be found from Discord's developer portal.
- `CONFESSIONS_ROLE_ID` is the role that grants permission to review confessions.

Set the "Interactions Endpoint URL" in Discord developer portal to `{base_url}/interaction`.

Setup your submission form to make a `POST /confession` every time a new confession is received.
For a Google Forms + Google Sheets setup, assuming the submission timestamp is column A and the content
is column B:

```js
// Create a trigger:
// - function: onFormSubmit
// - event source: spreadsheet
// - event type: on form submit
function onFormSubmit(e) {
  const payload = {
    index: e.range.getRow() - 5,
    content: e.values[1],
    timestamp: new Date(e.values[0]).toISOString(),
  };
  
  const options = {
    method: "POST",
    contentType: "application/json",
    payload: JSON.stringify(payload)
  };
  Logger.log(payload);

  UrlFetchApp.fetch("{base_url}/confession", options);
}
```

Sync commands with: `cfsbot2 register-commands`, run the bot with `cfsbot2`.
