---
title: Fetch Extension
description: Add Fetch MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

<YouTubeShortEmbed videoUrl="https://www.youtube.com/embed/_WMm4kDYMog" />

:::warning Known Limitation
The Fetch extension [does not work](https://github.com/aaif-goose/goose/issues/1184) with Google models (e.g. gemini-2.0-flash) because this extension uses `format: uri` in its JSON schema which Google doesn't support.
:::

This tutorial covers how to add the [Fetch MCP Server](https://github.com/modelcontextprotocol/servers/tree/main/src/fetch) as a goose extension to retrieve and process content from the web.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=uvx&arg=mcp-server-fetch&id=fetch&name=Fetch&description=Web%20content%20fetching%20and%20processing%20capabilities)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  uvx mcp-server-fetch
  ```
  </TabItem>
</Tabs>
:::

## Configuration

:::info
Note that you'll need [uv](https://docs.astral.sh/uv/#installation) installed on your system to run this command, as it uses `uvx`.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseDesktopInstaller
    extensionId="fetch"
    extensionName="Fetch"
    description="Web content fetching and processing capabilities"
    command="uvx"
    args={["mcp-server-fetch"]}
  />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Fetch"
      description="Web content fetching and processing capabilities"
      command="uvx mcp-server-fetch"
    />
  </TabItem>
</Tabs>

## Example Usage

Let's use the Fetch extension to get random programming jokes from the [Official Joke API](https://official-joke-api.appspot.com/random_joke).

:::info
While this API does not require an API key, many APIs do. If you need to provide an API key, you can set it as an environment variable and tell goose its name. Example: _"get tomorrow's weather for New Orleans from the weatherapi. Don't use the key directly but instead use a script that gets the value from the env var WEATHER_API_KEY"_
:::

### goose Prompt
```
make a web page that tells jokes for programmers. The page should ask the user the setup of the joke, allow them to guess the punchline and then tell them if they are right or wrong. Include a button that allows them to request another joke. you can fetch the jokes from the official joke api
```

### goose Output

```
I'll help you create a web page that uses the Official Joke API to display programming jokes. I'll break this down into steps:

First, let's verify the API endpoint and see what kind of data we get
Then create an HTML file with the necessary structure
Add JavaScript to handle the interactions

Let me check the API first:


FETCH
url: https://official-joke-api.appspot.com/jokes/programming/random

Output
Content type application/json; charset=utf-8 cannot be simplified to markdown, but here is the raw content:
Contents of https://official-joke-api.appspot.com/jokes/programming/random:
[{"type":"programming","setup":"A SQL query walks into a bar, walks up to two tables and asks...","punchline":"'Can I join you?'","id":23}]

Perfect! Now I'll create an HTML file with the necessary structure and JavaScript code:

TEXT_EDITOR
command: write
file_text:
<!DOCTYPE html> <html lang="en"> <head> <meta charset="U...
path:
programmer_jokes.html

Output
programmer_jokes.html


[... contents of programmer_jokes.html ...]

I've created a complete web page that:

Fetches random programming jokes from the Official Joke API
Shows the setup of the joke
Provides an input field for users to guess the punchline
Shows whether they got it right or wrong
Displays the actual punchline after they guess
Includes a button to get another joke

The page also includes:
Clean, modern styling
Error handling for API failures
Responsive design that works on both desktop and mobile
Visual feedback for correct/incorrect answers
Disabled input after guessing to prevent multiple attempts
You can open the file programmer_jokes.html in your web browser to try it out. Would you like me to help you test it or make any adjustments to the design or functionality?
```

### Results

![website goose built](../assets/guides/programmer-jokes-fetch-mcp.png)

<details>
<summary>Source Code</summary>

```html title="programmer_jokes.html"

<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Programmer Jokes</title>
    <style>
        body {
            font-family: Arial, sans-serif;
            max-width: 800px;
            margin: 0 auto;
            padding: 20px;
            text-align: center;
        }
        .container {
            background-color: #f5f5f5;
            padding: 20px;
            border-radius: 10px;
            margin-top: 20px;
        }
        .setup {
            font-size: 1.2em;
            margin-bottom: 20px;
        }
        .guess-container {
            margin-bottom: 20px;
        }
        #guessInput {
            padding: 10px;
            width: 300px;
            margin-right: 10px;
        }
        button {
            padding: 10px 20px;
            background-color: #4CAF50;
            color: white;
            border: none;
            border-radius: 5px;
            cursor: pointer;
        }
        button:hover {
            background-color: #45a049;
        }
        .result {
            margin-top: 20px;
            font-weight: bold;
        }
        .correct {
            color: green;
        }
        .incorrect {
            color: red;
        }
        #punchline {
            display: none;
            margin-top: 20px;
            font-style: italic;
        }
    </style>
</head>
<body>
    <h1>Programmer Jokes</h1>
    <div class="container">
        <div id="setup" class="setup">Loading joke...</div>
        <div class="guess-container">
            <input type="text" id="guessInput" placeholder="Guess the punchline...">
            <button onclick="checkGuess()">Submit Guess</button>
        </div>
        <div id="result" class="result"></div>
        <div id="punchline"></div>
        <button onclick="getNewJoke()" style="margin-top: 20px;">Get Another Joke</button>
    </div>

    <script>
        let currentJoke = null;

        async function getNewJoke() {
            try {
                const response = await fetch('https://official-joke-api.appspot.com/jokes/programming/random');
                const jokes = await response.json();
                currentJoke = jokes[0];
                
                document.getElementById('setup').textContent = currentJoke.setup;
                document.getElementById('guessInput').value = '';
                document.getElementById('result').textContent = '';
                document.getElementById('punchline').style.display = 'none';
                document.getElementById('guessInput').disabled = false;
            } catch (error) {
                document.getElementById('setup').textContent = 'Error loading joke. Please try again.';
            }
        }

        function checkGuess() {
            const guess = document.getElementById('guessInput').value.trim().toLowerCase();
            const punchline = currentJoke.punchline.toLowerCase();
            const resultElement = document.getElementById('result');
            const punchlineElement = document.getElementById('punchline');

            if (guess === punchline) {
                resultElement.textContent = 'Correct! You got it! 🎉';
                resultElement.className = 'result correct';
            } else {
                resultElement.textContent = 'Not quite! Here\'s the punchline:';
                resultElement.className = 'result incorrect';
            }

            punchlineElement.textContent = currentJoke.punchline;
            punchlineElement.style.display = 'block';
            document.getElementById('guessInput').disabled = true;
        }

        // Load first joke when page loads
        getNewJoke();
    </script>
</body>
</html>
```

</details>
