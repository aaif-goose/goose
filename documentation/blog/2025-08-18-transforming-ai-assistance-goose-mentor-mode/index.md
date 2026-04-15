---
title: "Transforming AI Assistance from Automation to Education: The Story Behind Goose Mentor Mode"
description: "How a Junior developer's AI Agent confusion, led to creating an educational MCP extension that transforms Goose from an automation tool into a learning mentor."
authors:
  - jeuston
---

![Goose Mentor Mode Header](goose-mentor-mode-header.png)

Kim is fresh out of the academy and has spent only 18 months learning development. When I asked her how she felt about Goose she had mixed reactions. While she found it cool that it could do so much for her, she wasn’t actually sure of *what* it was doing for her half the time, and also *why*. When she asked Goose to fix a broken build, or chase a bug, It would complete the task and claim ‘Success\!’. Which is great, however she felt she wasn’t actually learning as much as when she was in the academy. Add on that sometimes she didn’t even know *what* to ask Goose to do sometimes.

That afternoon I started to see if I could get Goose to be more than just a ‘magic box’ for my Junior Devs. What if Goose could instead act as a mentor and also teach as well as speeding up development?

<!-- truncate -->

# **Transforming AI Assistance from Automation to Education: The Story Behind Goose Mentor Mode**

Working as an Engineering Manager in the enterprise space, I have around 16 developers in my team. Like a lot of the industry right now I’ve been seeing where AI can fit within our processes, and what efficiencies it can provide. In July of this year I picked up Goose and quickly saw the incredible potential it had for both myself and my team. Excited to see what was possible, I quickly granted access to all my developers, hooked them up with a few different models, and then let them play for a couple of weeks.

Within my team I have a vast range of developer experience. From Tech Leads, all the way down to fresh graduates. The Seniors and Tech Leads were marvelling at the speed with which they could now progress their work. My mid level Devs were also amazed at how fast they could debug broken builds. It wasn’t until one of my regular catch up meetings with one of my Junior Graduates did I hear something different.

## **The Problem: AI That Does Instead of Teaches**

Traditional AI coding assistants operate on a simple premise: user asks, AI delivers. While this maximizes immediate productivity, it creates several long-term problems:

* **Dependency Development**: Developers become reliant on AI for problems they should understand  
* **Lost Learning Opportunities**: Every request could be a chance to build knowledge, but instead becomes just task completion  
* **One-Size-Fits-All**: No differentiation between a junior developer learning authentication and a senior developer implementing it under deadline pressure  
* **Context Blindness**: AI treats "how do I implement JWT?" the same whether it's asked by someone with 6 months or 6 years of experience

## **The Vision: Goose Mentor Mode**

The mission behind Goose Mentor Mode is simple but transformative: **Transform AI assistance from automation to education while maintaining the efficiency developers need**.

The core philosophy centers on four principles:

1. **Discovery Over Delivery**: Help users understand *why*, not just *how*  
2. **Adaptive Learning**: Adjust approach based on experience and context  
3. **Progressive Complexity**: Build understanding layer by layer  
4. **Retention Focus**: Emphasize learning that sticks

## **Current Features: A Proof of Concept**

The core of how I envision the system functioning is by configuring some basic assistance levels:

### **🎯 Four Adaptive Assistance Levels**

Think of the Assistance Modes as a dial that can balance the speed of learning against the speed of delivery.

**GUIDED Mode** \- For deep learning through discovery

* Uses Socratic questioning to guide users to solutions  
* Perfect for new concepts and skill building  
* *Example*: "What do you think JWT stands for? How might stateless authentication work?"

**EXPLAINED Mode** \- Education with implementation

* Provides detailed explanations alongside working code  
* Ideal for time-sensitive tasks with learning value  
* *Example*: "Here's how JWT works... \[detailed explanation\] \+ working code"

**ASSISTED Mode** \- Quick help with learning context

* Direct assistance with educational insights  
* Best for experienced developers needing quick help  
* *Example*: "Use this JWT library. Key security considerations: \[brief points\]"

**AUTOMATED Mode** \- Efficient task completion

* Direct solutions without educational overhead
* For production pressure and repeated tasks
* *Example*: "Here's the complete JWT implementation."

### 

### **🧠 Learning Detection**

Right now the system is in the PoC phase so I’m only using keyword checks for learning detection. I’m experimenting with semantic analysis to see if I can ‘intelligently’ do this, but that may prove overkill, or even bloat and slow down the system.

* **19 Technical Concepts** across 7 categories (security, database, API, architecture, testing, performance, DevOps)  
* **6 Intent Categories** for understanding request types (help requests, learning inquiries, debugging, etc.)  
* **Context-Aware Analysis** that distinguishes between "authentication error" vs "authentication best practices"

### **📊 Comprehensive Progress Tracking**

Ideally I can have long running progress tracking for each individual user that tracks over time. Right now this tracking is basic and only based on the current concept being ‘taught’.  
Some features of a fully implemented solution may include:

* Learning velocity tracking across concepts  
* Skill gap identification based on request patterns  
* Personalized learning path recommendations  
* Knowledge retention analysis over time

### **⚙️ Developer-Centric Configuration**

Easy setup through environment variables that integrate seamlessly with Goose Desktop:

```bash
DEFAULT_ASSISTANCE_LEVEL=guided          # Customize default behavior
LEARNING_PHASE=skill_building           # Set learning context
TIMELINE_PRESSURE=low                   # Adjust for project pressure
ENABLE_VALIDATION_CHECKPOINTS=true     # Control learning validation
DEVELOPER_EXPERIENCE_MONTHS=6           # Personalize experience
```

## 

## **Future Features: The Roadmap Ahead**

This being just a Proof of Concept it is very early days for the extension. I currently have a couple of my junior developers testing it out and providing feedback. Below is a list of ideas I’ve put together with help from Goose on what a future roadmap may look like.

### **Phase 1: Enhanced Intelligence (In Progress)**

* **Multi-Signal Learning Detection**: Combining semantic analysis, intent classification, and behavioral patterns  
* **Adaptive Thresholds**: Self-tuning confidence scoring based on user feedback  
* **Context-Aware Mentoring**: Decision engine that factors in user profile, project pressure, and learning phase

### **Phase 2: External Learning Integration**

* **Contextual Documentation Links**: Automatic linking to relevant documentation both online or possibly backed by Enterprise systems such as Confluence.  
* **Tutorial Recommendations**: Personalized learning path suggestions  
* **Best Practice Libraries**: Code pattern examples and educational resources

### **Phase 3: Advanced Analytics**

* **Learning Velocity Tracking**: Measure skill development across concepts  
* **Team Insights**: Collaborative learning opportunities and knowledge sharing  
* **Skill Gap Analysis**: Identify areas for focused learning  
* **Dynamic Assistance Adjustment**: Real-time adaptation based on learning progress

### **Phase 4: Team Coordination**

* **Multi-Developer Insights**: Team knowledge mapping and skill distribution  
* **Collaborative Learning**: Peer learning recommendations  
* **Knowledge Sharing**: Team-wide pattern recognition and best practices  
* **Privacy-Preserving Analytics**: Aggregated insights with individual privacy protection

## **Community Impact**

Although I did come up with the idea while at work, I quickly decided this would be a personal project outside of my organisation. This feels like something that should ideally be open source and open for wider adoption if it has merit. Right now the source code is available on github, as well as on PyPI for download.

## **The Bigger Picture: Changing How We Think About AI**

Goose Mentor Mode represents more than just a new extension—it's a proof of concept for a fundamentally different relationship between developers and AI. Instead of creating dependency, it builds capability. Instead of providing fish, it teaches fishing.

The early results suggest this approach resonates with developers who want to grow, not just get things done. As AI becomes more prevalent in software development, tools like Goose Mentor Mode point toward a future where AI enhances human capability rather than replacing human thinking.

---
*Goose Mentor Mode is open source and available on [PyPI](https://pypi.org/project/goose-mentor-mode/). Join the conversation on [GitHub](https://github.com/joeeuston-dev/goose-mentor-mode) and help shape the future of educational AI assistance.*

<head>
  <meta property="og:title" content="Transforming AI Assistance from Automation to Education: The Story Behind Goose Mentor Mode" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/08/14/transforming-ai-assistance-gooe-mentor-mode" />
  <meta property="og:description" content="How a Junior developer's AI Agent confusion, led to creating an educational MCP extension that transforms Goose from an automation tool into a learning mentor." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/goose-mentor-mode-header-77058a250a163440d791e057ef3196ea.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="Transforming AI Assistance from Automation to Education: The Story Behind Goose Mentor Mode" />
  <meta name="twitter:description" content="How a Junior developer's AI Agent confusion, led to creating an educational MCP extension that transforms Goose from an automation tool into a learning mentor." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/goose-mentor-mode-header-77058a250a163440d791e057ef3196ea.png" />
</head>