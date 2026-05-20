---
title: Using Subagents
description: Spin up a team of subagents to build a fully functional app
image: /assets/images/tutorial-using-subagents-ef265627024db73e73d80e5799ed0c1a.png
---

<div style={{display: 'none'}}>
![](/img/tutorial-using-subagents.png)
</div>

This tutorial walks you through how to spin up a team of AI [subagents](/docs/guides/context-engineering/subagents/) and guide them through building a fully functional app.

You'll build **AI BriefMe**, an app that generates a structured executive style briefing based on any topic. 

You'll use goose to orchestrate a full software team of subagents:

- 🧠 **Planner** – defines the product and MVP scope  
- 📋 **Project Manager** – breaks down tasks and coordinates execution  
- 🏗️ **Architect** – sets up the project structure and tech stack  
- 💻 **Frontend and Backend Devs** – build the interface and API logic  
- 🧪 **QA** – writes tests and flags bugs or production blockers  
- 📝 **Tech Writer** – documents setup, usage, and API details  

By the end of the session, you'll have a working prototype and a clear understanding of how to use AI agents for realistic workflows.

## Setup

1. [Install goose](/docs/getting-started/installation)
2. Within goose, choose your working directory. It's recommended to work in a newly created directory.
3. Add the following to [.goosehints](/docs/guides/context-engineering/using-goosehints/#local-hints-file)
```plaintext
Create apps in html, javascript, and css when possible.
NEVER run blocking server commands (node server.js, npm start, etc.) - provide commands for user to run separately.
```
4. (Optional) Install the [goose docs extension](/docs/mcp/goose-docs-mcp) in case you need to ask goose about itself

## Tasks

You'll be building an AI-powered briefing app by spinning up a team of subagents to help you. Each agent has a clear role. Your job is to figure out how to prompt goose to delegate the work. 

> 🛟 If you get stuck, you can peek to the prompts for examples.

---

### 1. 🧠 Planner

Have the Planner define the product vision and scope. The Planner should decide:
- What the app is
- Who it's for
- What problem it solves
- The core features for the MVP

This output should be a clear product definition, not code.

<details>
  <summary>Planner Agent Prompt</summary>
  
  ```md
  You're the Planner agent for a hands-on AI app building session using goose and subagents. We are building the MVP *right now*.

  The app is called **AI BriefMe**. It generates a daily briefing on any given topic. A user inputs a topic like “Apple earnings” or “AI in DevOps,” and the app returns:
  - A title
  - Today's date
  - 2–3 bullet-point takeaways
  - (Optional) a code snippet or chart, if the topic is technical

  You're working with a team of subagents — PM, Architect, Frontend Dev, Backend Dev, QA, and Tech Writer — who will immediately begin executing your plan.

  Write a short, focused **Markdown file (`plan.md`)** that outlines:
  - The goals of the MVP
  - Only the features that can be built in a 40 minute session
  - Any helpful design considerations

  ✅ DO: Keep it lean and actionable  
  ❌ DON'T: Include long-term features like email delivery, user accounts, dashboards, analytics, personalization, mobile optimization, or 8-week timelines
  ```
</details>

---

### 2. 📋 Project Manager

Ask the Project Manager to break the product down into development tasks. The output should:
- Define which roles are needed (e.g. frontend, backend)
- List the specific tasks and assign them
- Indicate which tasks can happen in parallel vs sequentially

<details>
  <summary>PM Agent Prompt</summary>
  
  ```md
  You're the PM agent. A Planner has just created `plan.md` for a 1-hour build session of an app called "AI BriefMe."

  Your job is to:
  - Break the work into tasks for each subagent: Architect, Backend Dev, Frontend Dev, QA, Tech Writer
  - Group tasks by agent
  - Decide what work can be done in parallel vs what must be sequential
  - Output the task breakdown in Markdown and save it as `project_board.md`

  Be realistic and concise — this is a sprint, not a roadmap.
  ```
</details>

---

### 3. 🏗️ Architect

Have the Architect plan the technical setup. They should:
- Choose the stack (frontend + backend frameworks, any libraries)
- Describe folder structure and file organization
- Flag anything you'll need to install (and optionally provide install commands)

<details>
  <summary>Architect Agent Prompt</summary>
  
  ```md
  You are the Architect. Based on the project plan and `project_board.md`, set up the project scaffolding.

  Do the following:
  - Create the folder structure and all placeholder files (e.g. `index.html`, `server.js`, `style.css`, etc.)
  - Generate a `package.json` file that includes `express`, `cors`, and `child_process` as dependencies
  - Add a `.gitignore` that excludes `node_modules` and any temporary files
  - Define the API contract for the `/api/briefing` endpoint in Markdown

  ✅ Do NOT include or reference any API keys  
  ✅ Do NOT install packages — just scaffold the structure  
  ✅ DO list the output files and folders at the end

  ```
</details>

---

### 4. 💻 Frontend + Backend Developers (Parallel)

Spin up two developer subagents in parallel to build the core app. One will handle the Express server and backend logic, the other will build the UI and wire up the form. goose should execute both agents at the same time, not one after the other.

- Use the file structure and API contract from the Architect
- Backend writes the API logic using Headless goose
- Frontend builds a responsive UI that hits the API
- Make sure agents avoid writing to the same files

<details>
  <summary>Dev Agents Prompt</summary>
  
  ```md
  Use **parallel execution** to run two subagents:

  - A 🛠️ Backend Developer
  - A 💻 Frontend Developer

  Both should work simultaneously — not sequentially — to build the AI BriefMe MVP.

  🛠️ **Backend Developer** should:
  - Implement `server.js` with Express
  - Add POST `/api/briefing` endpoint accepting `{ "topic": "string" }`
  - Use **Headless goose** to generate the summary:
    - `goose run -t "YOUR_PROMPT_HERE" --quiet --no-session --max-turns 1`
  - Use `child_process.spawn()` instead of `exec()`
  - Clean response: remove ANSI codes, markdown blocks, and extract JSON
  - Handle timeouts (max 60s) and errors
  - Serve frontend files from `express.static`
  - Add CORS
  - Do not require any API keys

  💻 **Frontend Developer** should:
  - Create `index.html`, `style.css`, and `script.js`
  - Build a form for entering a topic
  - Call the `/api/briefing` endpoint and display the result
  - Handle loading states and errors
  - Include a copy-to-clipboard button
  - Make it mobile-friendly
  - Do not interfere with backend files

  ⚠️ Important: These agents must not write to the same files. Keep their work isolated.

  ```
</details>

---

### 5. 🧪📝 QA + Tech Writer (Parallel)

Now that development is done, spin up two final subagents: a **QA Engineer** and a **Tech Writer**. They'll work together to assess the app's quality and document how to use it. Your job is to prompt goose in a way that gets both agents to collaborate without overlapping or duplicating work.

#### QA Agent Tasks:
- Write a unit test suite for the `/api/briefing` endpoint using a framework like Jest
- Create a `QA_NOTES.md` file:
  - Flag bugs or edge cases
  - Identify production-readiness blockers (e.g. security, error handling)
  - Recommend next steps to improve reliability

#### Tech Writer Tasks:
- Collaborate with QA to understand the current state of the app
- Create a `README.md` file with:
  - What the app does (in plain language)
  - How to install and run it
  - Example API usage

⚠️ Important:
- **QA should not start the server manually** — just write test files and mock interactions


<details>
  <summary>QA Agent + 📝 Tech Writer Agent Prompt (Parallel)</summary>
  
  ```md
  The development phase is complete. Now it's time for quality assurance and documentation.

  Use **parallel execution** to run two subagents simultaneously:

  - 🧪 A **QA Agent** who will:
    - Write a unit test for the `/api/briefing` endpoint in `tests/briefing.test.js` using Jest
    - **Mock the child_process module** using `jest.mock('child_process')` at the top of the test file
    - Create a simple mock that returns fake data instead of calling the real goose CLI
    - Assert that the response includes: `title`, `date`, and 2–3 `takeaways`
    - Include tests for:
      - Valid topic input
      - Missing or invalid input
      - goose CLI timeout or error
    - **Do not start or run the server manually.** Only write test files.
    - **Do not execute `npm test` or run any tests.** Only create the test file.
    - Save a full QA analysis report in `QA_NOTES.md` with:
      - Critical issues
      - Security or performance gaps
      - Recommendations for production readiness
    - **When all files are created, immediately state: "QA Agent Sign-off: ✅ COMPLETE" and finish.**

  - 📝 A **Tech Writer Agent** who will:
    - Create a `README.md` that includes:
      - Project overview
      - How to run the app locally
      - API endpoint documentation
      - Example request/response
      - Troubleshooting section
    - **When documentation is complete, immediately state: "Tech Writer Sign-off: ✅ COMPLETE" and finish.**

  Both agents should work in parallel, not sequentially.
  ```
</details>

---

## Test Your Completed App

Once all agents complete their work, you should have a working prototype. Here's how to see it in action:

### Step 1: Install Dependencies
```bash
cd your-project-folder
npm install
```

### Step 2: Start the Server
**Important**: Run this in a **separate terminal window** (not in goose):
```bash
npm start
```

You should see:
```
AI BriefMe server running on port 3000
Health check: http://localhost:3000/health
Briefing endpoint: http://localhost:3000/api/briefing
```

### Step 3: Open the App
Open your browser and go to:
```
http://localhost:3000/
```

**Note**: Use the root URL (`/`), not `/ai-briefme/`

### Step 4: Test It
1. Enter a topic like "JavaScript frameworks" or "climate change"
2. Click "Get Briefing"
3. Watch the AI generate your briefing!

You should see:
- A clean, responsive interface
- AI-generated briefings with title, date, and takeaways
- Code examples for technical topics
- Copy-to-clipboard functionality

:::tip Keep the Server Running
- **Don't close the terminal** where the server is running
- **Don't run the server in goose** - it will get stuck
- If you need to stop it: Press `Ctrl+C` in the server terminal
- If you need goose to fix or add something, let it know! Once it's done, restart the server
:::

**Congratulations! You've built a full-stack AI app using goose subagents!** 🎉

:::warning
Don't expect your app to be production ready. This workshop shows how vibe coding with goose can accelerate prototyping, but the human still owns the judgment and polish.
:::

---

## Troubleshooting

### Cannot POST /api/briefing (404 Error)
**Cause**: Route not registered or server not restarted
**Solutions**:
1. **Stop your server** (Ctrl+C in terminal)
2. **Restart the server**: `node server.js`
3. **Test with curl**:
   ```bash
   curl -X POST http://localhost:3000/api/briefing -H "Content-Type: application/json" -d '{"topic":"test"}'
   ```
4. **Should return JSON, not HTML**

---

### Unexpected token

**Cause**: Frontend receiving HTML instead of JSON (usually a 404 page)
**Solutions**:
1. **First, test the API directly**:
   ```bash
   curl -X POST http://localhost:3000/api/briefing -H "Content-Type: application/json" -d '{"topic":"test"}'
   ```
2. **If you get HTML back**: Your API route isn't working (see Issue 1)
3. **If you get JSON back**: Check browser Network tab to see what URL the frontend is actually calling
4. **Common fix**: Make sure you're accessing the app at `http://localhost:3000/` not `/ai-briefme/`

---

### Process Timeout
**Cause**: goose taking too long or hanging
**Solutions**: 
1. **Check your goose command flags**:
   ```javascript
   ['run', '-t', prompt, '--quiet', '--no-session', '--max-turns', '1']
   ```
2. **Test goose manually**:
   ```bash
   goose run -t "Return JSON: {\"test\": \"value\"}" --quiet --no-session --max-turns 1
   ```
3. **If manual test works**: Check your spawn() implementation
4. **If manual test hangs**: Try a simpler prompt first

---

### JSON Parsing Errors
**Cause**: goose returns formatted output with color codes
**Solutions**:
1. **Add this cleaning code** before JSON.parse():
   ```javascript
   // Remove ANSI color codes
   jsonString = jsonString.replace(/\x1b\[[0-9;]*m/g, '');
   // Remove markdown formatting
   jsonString = jsonString.replace(/```json\s*/, '').replace(/```\s*$/, '');
   jsonString = jsonString.replace(/```\s*/, '');
   ```
2. **Add debug logging** to see what you're getting:
   ```javascript
   console.log('Raw goose response:', aiResponse);
   console.log('Cleaned JSON string:', jsonString);
   ```

---

### Port Already in Use (EADDRINUSE)
**Solutions**:
1. **Find what's using port 3000**:
   ```bash
   lsof -ti:3000
   ```
2. **Kill the process**:
   ```bash
   lsof -ti:3000 | xargs kill -9
   ```
3. **Or use a different port**:
   ```javascript
   const PORT = process.env.PORT || 3001;
   ```
---

### App Shows Blank Page
**Solutions**:
1. **Check you're at the right URL**: `http://localhost:3000/` (not `/ai-briefme/`)
2. **Check browser console** for JavaScript errors
3. **Verify static files are served**:
   ```javascript
   app.use(express.static(__dirname));
   ```
4. **Test individual files**:
   - `http://localhost:3000/index.html`
   - `http://localhost:3000/style.css`
   - `http://localhost:3000/script.js`

