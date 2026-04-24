---
title: How 7 AI Agents Worked Together to Build an App in One Hour
description: "Learn how to build a full-stack AI app in under an hour using Goose's subagent orchestration, from planning to testing."
authors: 
    - angie
---

![blog cover](header-image.png)

What if I told you that you could build a complete, working web application in under an hour using nothing but AI agents? Not just a simple "Hello World" app, but a full-stack application with a backend API, responsive frontend, unit tests, and documentation?

That's exactly what we accomplished during the Vibe Coding workshop at [Berkeley's Agentic AI Summit](https://www.youtube.com/live/_w5m3h9jY-w?t=5310), where I demonstrated how to use Goose's subagent orchestration to spin up an entire development team of AI agents. Each agent took on a specific role - from product planning to QA testing - and worked together to build "AI BriefMe", a web app that generates executive style briefings on any topic.

<!-- truncate -->

## The Power of Multi-Agent Development

Traditional AI coding assistants are great at helping you write individual functions or debug specific issues. But what if you need to build something from scratch? What if you want to simulate an entire software development lifecycle?

That's where Goose's subagent feature shines. Instead of doing everything yourself, you can orchestrate a team of specialized AI agents, each with their own expertise:

- 🧠 **Planner** - Defines the product vision and MVP scope
- 📋 **Project Manager** - Breaks down tasks and coordinates execution  
- 🏗️ **Architect** - Sets up project structure and tech stack
- 🎨 **Frontend Developer** - Builds the interface
- 🧩 **Backend Developer** - Builds the API logic
- 🧪 **QA Engineer** - Writes tests and identifies production blockers
- 📝 **Tech Writer** - Documents setup, usage, and API details

## The Workshop Experience

During the live workshop, participants followed along as we built AI BriefMe step by step. The beauty of this approach is that you're not just watching someone code, you're learning how to prompt and orchestrate AI agents effectively.

Here's how the workflow unfolded:

### Step 1: Product Planning
First, we spun up a Planner agent to define what we were building. Instead of diving straight into code, the Planner created a clear product definition:

<details>
 <summary>Product Plan</summary>

 ```md
 # AI BriefMe MVP - 40-Minute Build Plan

## Goals
Build a functional web app that generates daily briefings on any topic in **40 minutes**. Users input a topic and get an instant, well-formatted briefing.

## Core MVP Features (Must-Have)
1. **Simple web interface** with topic input field and generate button
2. **AI-powered briefing generation** that returns:
   - Title
   - Today's date
   - 2-3 bullet-point takeaways
   - Optional code snippet or chart for technical topics
3. **Clean display** of the generated briefing
4. **Basic error handling** for API failures

## Technical Stack (Keep It Simple)
- **Frontend**: Single HTML page with vanilla JS (no frameworks)
- **Backend**: Python Flask app with single endpoint
- **AI**: Headless Goose as an LLM service
- **Deployment**: Local development server (no cloud deployment)

## Team Responsibilities

### PM
- Define exact briefing format and user flow
- Create sample topics for testing

### Architect  
- Design simple API contract between frontend/backend
- Choose AI prompt structure for consistent output

### Frontend Dev
- Build single-page interface with form and results display
- Handle loading states and basic error messages

### Backend Dev
- Create Flask app with `/generate-briefing` endpoint
- Integrate with AI API and format response
- Add basic input validation

### QA
- Test with 3-5 different topic types
- Verify error handling works
- Check output format consistency

### Tech Writer
- Write brief README with setup instructions
- Document the API endpoint

## Design Considerations
- **Mobile-friendly** but desktop-first
- **Fast response time** - show loading indicator
- **Copy-friendly output** - users should be able to easily copy/share
- **Graceful failures** - clear error messages when AI is unavailable

## Success Criteria
✅ User can enter any topic and get a formatted briefing  
✅ App handles both technical and non-technical topics  
✅ Clean, readable output format  
✅ Works locally without deployment complexity  

## Out of Scope (Save for Later)
- User accounts or login
- Email delivery or scheduling  
- Historical briefings or dashboard
- Advanced formatting or customization
- Mobile app or PWA features
- Analytics or usage tracking

---
**Timeline**: 40 minutes total  
**Demo ready**: Functional app running locally with 2-3 example briefings generated
```
</details>


### Step 2: Project Management
Next, the Project Manager agent broke down the work into specific tasks, identifying what could be done in parallel versus sequentially

<details>
  <summary>Project Board</summary>

  ```md
  # AI BriefMe - Project Board

## Sprint Overview
**Duration**: 40 minutes  
**Goal**: Functional MVP with topic input → AI briefing generation → display

---

## 🏗️ ARCHITECT (Start First - 5 minutes)
**Dependencies**: None - blocks all other dev work

### Tasks:
- [ ] **API Contract Design** (3 min)
  - Define `/generate-briefing` POST endpoint structure
  - Specify request/response JSON format
  - Document error response codes
- [ ] **AI Prompt Template** (2 min)
  - Create consistent prompt structure for briefing generation
  - Define output format requirements (title, date, bullets, optional code)

**Deliverables**: `api_spec.md` with endpoint docs and prompt template

---

## 🔧 BACKEND DEV (After Architect - 15 minutes)
**Dependencies**: API contract from Architect

### Tasks:
- [ ] **Flask App Setup** (3 min)
  - Create `app.py` with basic Flask structure
  - Add CORS for frontend integration
- [ ] **Generate Briefing Endpoint** (8 min)
  - Implement `/generate-briefing` POST route
  - Format AI response to match API contract
- [ ] **Error Handling** (2 min)
  - Add try/catch for API failures
  - Return appropriate error responses
- [ ] **Basic Validation** (2 min)
  - Validate topic input (not empty, reasonable length)
  - Sanitize input before sending to AI

**Deliverables**: Working Flask backend ready for frontend integration

---

## 🎨 FRONTEND DEV (Parallel with Backend - 15 minutes)
**Dependencies**: API contract from Architect (can start with mock data)

### Tasks:
- [ ] **HTML Structure** (3 min)
  - Create `index.html` with form and results sections
  - Add basic semantic structure
- [ ] **CSS Styling** (5 min)
  - Style input form and results display
  - Add loading spinner/state
  - Make mobile-friendly
- [ ] **JavaScript Logic** (5 min)
  - Handle form submission
  - Make API call to backend
  - Display results and handle loading states
- [ ] **Error UI** (2 min)
  - Show user-friendly error messages
  - Handle network failures gracefully

**Deliverables**: Complete frontend ready to connect to backend

---

## 🧪 QA (After Backend + Frontend Ready - 8 minutes)
**Dependencies**: Working backend and frontend integration

### Tasks:
- [ ] **Happy Path Testing** (3 min)
  - Test 3 different topic types: business, technical, general
  - Verify output format consistency
- [ ] **Error Scenarios** (3 min)
  - Test empty input, very long input
  - Test with backend down/API key issues
  - Verify error messages display correctly
- [ ] **Cross-browser Check** (2 min)
  - Quick test in Chrome and Safari
  - Verify mobile responsiveness

**Deliverables**: Bug report and sign-off for demo readiness

---

## 📝 TECH WRITER (Parallel with Development - 10 minutes)
**Dependencies**: API spec from Architect, can work in parallel

### Tasks:
- [ ] **README Creation** (5 min)
  - Setup instructions for local development
  - Required dependencies and API keys
  - How to run the application
- [ ] **API Documentation** (3 min)
  - Document the `/generate-briefing` endpoint
  - Include request/response examples
- [ ] **Sample Topics List** (2 min)
  - Create 5-10 example topics for demo
  - Mix of technical and non-technical subjects

**Deliverables**: `README.md`, `API.md`, and `sample_topics.md`

---

## 📋 PM COORDINATION TASKS
**Ongoing throughout sprint**

### Tasks:
- [ ] **Define Exact Briefing Format** (2 min)
  - Specify title format, bullet structure
  - Decide on code snippet criteria
- [ ] **Create Test Topics** (3 min)
  - Prepare demo scenarios
  - Include edge cases for QA
- [ ] **Integration Coordination** (5 min)
  - Ensure frontend/backend connection works
  - Coordinate final testing and demo prep

---

## ⏱️ TIMELINE & DEPENDENCIES

### Phase 1 (0-5 min): Foundation
- **Architect**: API design and prompt template
- **Tech Writer**: Start README and documentation

### Phase 2 (5-20 min): Parallel Development  
- **Backend Dev**: Build Flask app and endpoint
- **Frontend Dev**: Build UI (can use mock data initially)
- **Tech Writer**: Continue documentation

### Phase 3 (20-32 min): Integration & Testing
- **Frontend/Backend**: Connect and test integration
- **QA**: Begin testing as soon as integration works
- **PM**: Coordinate final pieces

### Phase 4 (32-40 min): Final Polish & Demo Prep
- **All**: Bug fixes and demo preparation
- **QA**: Final sign-off
- **PM**: Demo script and presentation

---

## 🎯 CRITICAL PATH
1. Architect completes API spec → Backend can start
2. Backend completes endpoint → Frontend integration can happen  
3. Frontend + Backend working → QA can test
4. QA passes → Demo ready

## ⚠️ RISK MITIGATION
- **Integration Problems**: Frontend dev should test with mock data first
- **Time Overruns**: Cut optional features (code snippets, advanced styling) if needed
```

</details>


### Step 3: Technical Architecture
The Architect agent established the technical foundation:

- **Stack**: Vanilla HTML/CSS/JS frontend, Express.js backend
- **API Design**: Simple POST endpoint accepting `{"topic": "string"}`
- **File Structure**: Organized project with clear separation of concerns
- **Dependencies**: Express, CORS, and child_process for calling Headless Goose

The Architect also defined the API contract, which made it possible for the next step where the frontend and backend developer agents can work in parallel.

### Step 4: Parallel Development
This is where things got really interesting. We spun up two developer agents simultaneously:

The **Frontend Developer** created:
- Clean, responsive interface with modern CSS
- Form handling with loading states
- Error handling and user feedback
- Copy-to-clipboard functionality

The **Backend Developer** implemented:
- Express server with proper error handling
- `/api/briefing` endpoint that uses Headless Goose for AI generation
- Response parsing and JSON formatting
- Timeout handling and CORS configuration

#### The Magic of Headless Goose

One of the coolest aspects of this project was how the backend used [Headless Goose](/docs/tutorials/headless-goose) by essentially calling Goose programmatically to generate the AI briefings:

```javascript
const gooseProcess = spawn('goose', [
  'run', '-t', prompt, 
  '--quiet', '--no-session', '--max-turns', '1'
]);
```

This creates a fascinating recursive scenario: we're using Goose to build an app that uses Goose to generate content. It's AI agents all the way down!

### Step 5: Testing and Docs
Finally, we ran QA and Tech Writer agents in parallel:

The **QA Engineer** delivered:
- Comprehensive unit test suite using Jest
- Mocked external dependencies for reliable testing
- Detailed analysis of production-readiness blockers
- Security and performance recommendations

The **Tech Writer** produced:
- Complete README with setup instructions
- API documentation with examples
- Troubleshooting guide
- Usage examples and best practices

## Real Results in Real Time

By the end of the hour, participants had a fully functional web application. Here's what the final product delivered:

- **Clean UI**: Professional-looking interface that works on desktop and mobile
- **AI-Powered Content**: Generates structured briefings with titles, dates, and key takeaways
- **Code Examples**: For technical topics, includes relevant code snippets
- **Production Insights**: QA analysis revealed specific areas needing attention before deployment
- **Complete Documentation**: Everything needed to run, modify, and extend the app

But here's the important part: this wasn't production-ready code. The QA agent was very clear about that, flagging security, performance, and scalability issues.

<details>
  <summary>QA Analysis Highlights</summary>

  ```md
    ## 🔍 QA Analysis Highlights

    ### Critical Issues Identified
    - **Security**: Command injection risk, no authentication, missing rate limiting
    - **Performance**: Blocking operations, memory leaks, inefficient parsing
    - **Scalability**: Single-threaded bottleneck, no horizontal scaling support

    ### Risk Assessment
    - **Overall Risk Level**: HIGH ⚠️
    - **Production Readiness**: Not recommended without addressing critical issues
    - **Timeline for Production**: 2-3 weeks for P0 items, 4-6 weeks for full readiness

    ### Testing Quality Assessment
    - **Test Coverage**: Excellent (91%+ across all metrics)
    - **Edge Case Handling**: Comprehensive
    - **Error Scenarios**: Well covered
    - **Resilience Testing**: Implemented
  ```

</details>

## The Human Still Matters

This workshop perfectly illustrates the current state of AI-assisted development. Goose and its subagents can absolutely accelerate prototyping and help you build working applications quickly. But the human developer still owns the critical judgment calls:

- **Architecture decisions**: Is this the right approach for the problem?
- **Security considerations**: What are the risks we need to mitigate?
- **Production readiness**: What needs to be hardened before real users touch this?
- **Business logic**: Does this actually solve the user's problem?

## The Future of Development

What we demonstrated in this workshop hints at a fascinating future for software development where we might find ourselves orchestrating AI agent teams. The skills that matter become:

- **Prompt engineering**: How do you communicate requirements clearly to AI agents?
- **System design**: How do you break complex problems into agent-sized tasks?
- **Quality assurance**: How do you validate and test AI-generated code?
- **Integration**: How do you combine outputs from multiple agents into cohesive solutions?

## Getting Started with Subagents

Want to try this yourself? Here's what you need:

1. **Install and Configure Goose**: Follow the [quickstart guide](https://goose-docs.ai/docs/quickstart)
2. **Start Small**: Try building a simple app first to get comfortable with the workflow

:::note
As of version 1.10.0, subagents are no longer experimental and don't require enabling any feature flags.
:::

The [complete workshop materials](https://gist.github.com/angiejones/60ff19c08c5a3992e42adc8de3e96309) are available, including step-by-step instructions and cheat sheet prompts. 

The key is learning how to prompt effectively. Each agent needs clear instructions about their role, constraints, and deliverables.

Remember, this is about prototyping and exploration, not production deployment. Use it to quickly validate ideas, create demos, or learn new technologies. Then apply human judgment to decide what's worth polishing into production-quality software.

---

*Want to see this in action? Check out the full workshop video where we build AI BriefMe live:*

<iframe class="aspect-ratio" src="https://www.youtube.com/embed/_w5m3h9jY-w?start=5310" title="Vibe Coding with Goose Workshop" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe>

<head>
  <meta property="og:title" content="How 7 AI Agents Worked Together to Build an App in One Hour" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/08/10/vibe-coding-with-goose-building-apps-with-ai-agents" />
  <meta property="og:description" content="Learn how to build a full-stack AI app in under an hour using Goose's multi-agent orchestration, from planning to QA testing." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/header-image-b685ea475ff7b8ae3563317b347fddb0.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="How 7 AI Agents Worked Together to Build an App in One Hour" />
  <meta name="twitter:description" content="Learn how to build a full-stack AI app in under an hour using Goose's multi-agent orchestration, from planning to QA testing." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/header-image-b685ea475ff7b8ae3563317b347fddb0.png" />
</head>
