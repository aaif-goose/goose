---
sidebar_position: 87
title: Creating Plans Before Working
sidebar_label: Creating Plans
---

import Card from '@site/src/components/Card';
import styles from '@site/src/components/Card/styles.module.css';

Starting a project without a clear plan is like building a house without a blueprint. It can lead to:

* Confusion about what to do
* Wasted time and effort
* Projects that grow too big

A good plan keeps everyone on track and helps measure progress. That's why the goose CLI includes the `/plan` prompt completion command to help break down your projects into clear, manageable steps.
 
:::tip Plans in the goose Desktop
The goose Desktop doesn't have a `plan` keyword. If you want goose Desktop to create a plan for you, you need to use a prompt like:

```
"Hey goose, can you create a plan to convert my CLI project into a locally hosted web page that gives me input fields for each CLI command I can run? Please don't start the actual work"
```
Unless you ask goose to "create a plan", it might just start into the project work. 
:::

The goose CLI's plan mode is interactive, asking clarifying questions to understand your project before creating a plan. If you can provide thoughtful and informative answers to those questions, goose can generate a really useful and actionable plan.

## Set your planner provider and model
In some workflows, it can be helpful to use one LLM for planning and a different one for execution. For example, GPT-4.1 tends to excel at strategic planning and breaking down complex tasks into clear, logical steps. On the other hand, Claude Sonnet 3.5 is particularly strong at writing clean, efficient code and following instructions precisely. By using GPT-4.1 to plan and Claude to execute, you can play to the strengths of both models and get better results overall.

The goose CLI plan mode uses two configuration values:

- `GOOSE_PLANNER_PROVIDER`: Which provider to use for planning
- `GOOSE_PLANNER_MODEL`: Which model to use for planning

:::tip Multi-Model Alternative to Plan Mode
goose also supports automatic model switching with [Lead/Worker mode](/docs/guides/environment-variables#leadworker-model-configuration), which provides turn-based switching between two models to help balance model capabilities with cost and speed.
:::

:::tip Customize Plan Format
You can also customize how goose creates plans by editing the `plan.md` [prompt template](/docs/guides/prompt-templates).
:::

### Set goose planner environment variables
You might add these lines to your bash shell config file (.bashrc) to add the planner environment variables:
```bash
export GOOSE_PLANNER_PROVIDER=<my-chosen-provider>
export GOOSE_PLANNER_MODEL=<my-chosen-model>
```
After you save your changes to the config file, you need to re-start your goose session so that goose can use the variables.

If these aren't set, goose will use your default provider and model settings. You might want to set different planning models if you find certain models are better at breaking down tasks into clear steps. However, your default model configuration is usually sufficient.

To verify that the planner provider is set, input the following terminal command:

```bash
~ goose info -v
```

In this example, the `info` command returns the current configuration and the path to the configuration file.  

```bash
goose Version:
  Version:          1.0.18

goose Locations:
  Config file:      /Users/alincoln/.config/goose/config.yaml
  Sessions dir:     /Users/alincoln/.local/share/goose/sessions
  Logs dir:         /Users/alincoln/.local/state/goose/logs

goose Configuration:
  GOOSE_PROVIDER: anthropic
  GOOSE_MODEL: claude-3.5-sonnet
  GOOSE_PLANNER_PROVIDER: openai
  GOOSE_MODE: smart_approve
  GOOSE_PLANNER_MODEL: gpt-4.1
```

 If either `GOOSE_PLANNER_PROVIDER` or `GOOSE_PLANNER_MODEL` are not set, `GOOSE_PROVIDER` and `GOOSE_MODEL`are used to build your plan.  

## Describe your project
While goose can handle complex project descriptions, it works best with clear, concise ones. Focus on stating your project's purpose and desired outcomes. If these aren't clear, goose will ask clarifying questions until it fully understands your goals. 

## A simple construction plan example
goose can produce good plans for relatively simple projects such as the home construction example:

```bash
( O)> /plan

Entering plan mode. You can provide instructions to create a plan and then act on it. To exit early, type /endplan

( O)> Build a four bedroom house
◓  Measuring semantic distance...                                                                                                         

I need to gather more information before creating a detailed house-building plan. Please provide clarification on the following:

1. What is the purpose of this plan? Are you looking for architectural designs, construction steps, or a project management timeline?
2. What is your budget range for this four-bedroom house?
3. What is the approximate square footage or size requirements?
4. Do you have a specific location or lot already selected? What are the dimensions and any zoning restrictions?
5. Are there any specific architectural styles or features you want included?
6. How many bathrooms and other rooms (e.g., kitchen, living room, dining room, garage) do you want?
7. Do you have any specific sustainability or energy efficiency requirements?
8. What is your timeline for completion?

( O)> 1 construction steps and a project management timeline. 2 $600,000. 3 4,000 sq feet. 4 the lot is not yet selected. 5 one story rambler. 6 four bedrooms, two ful baths, a kitchen, diningroom, family room, and garage. 7 the house must be efficient and powered by solar energy. 8 complete in six months. 
◓  Taming tensors...                                                                                                          I'll create a detailed plan for building a 4,000 sq ft, one-story rambler with four bedrooms, two full bathrooms, kitchen, dining room, family room, garage, and home office. The house will be energy efficient with solar power, with a $600,000 budget and 6-month timeline.

◇  Do you want to clear message history & act on this plan?
│  Yes 
│

Exiting plan mode and acting on the above plan

### Introduction to Building a 4,000 Sq Ft Rambler
To create a detailed plan for your 4,000 sq ft, one-story rambler, we need to consider several factors including design, energy efficiency, budget, and timeline. Given your requirements, here's an overview of how to approach this project:

### Step 1: Design and Planning
- **Architectural Design**: Hire an architect to design the house, ensuring it meets your space requirements and is energy efficient. Consider factors like natural lighting, insulation, and window placement.
- **Floor Plan**: Create a floor plan that includes four bedrooms, two full bathrooms, a kitchen, dining room, family room, garage, and home office. Ensure the layout is functional and spacious.
- **Land Preparation**: Ensure the building site is prepared, including clearing the land, grading, and preparing the foundation.

### Step 2: Energy Efficiency and Solar Power
- **Solar Power System**: Design and install a solar power system that can meet the energy needs of a 4,000 sq ft house. This may include a large solar panel array and a battery storage system for nighttime and cloudy days.
- **Insulation and Windows**: Use high-quality insulation in the walls, floor, and ceiling to minimize heat loss. Install energy-efficient windows that allow natural light in while keeping the heat or cold out.
- **Energy-Efficient Appliances**: Specify energy-efficient appliances and lighting to minimize energy consumption.

### Step 3: Budgeting
- **Construction Costs**: Allocate approximately $200-250 per sq ft for construction, which translates to $800,000-$1,000,000 for a 4,000 sq ft house. However, your budget is $600,000, so you'll need to optimize costs without compromising on quality and energy efficiency.
- **Solar Power System Costs**: Budget an additional $20,000 to $50,000 for the solar power system, depending on its size and complexity.
- **Contingency Fund**: Set aside 10-20% of the budget for unexpected expenses.

### Step 4: Timeline
- **Month 1-2: Design and Planning**: Complete the architectural design, obtain necessary permits, and prepare the building site.
- **Month 3-4: Foundation and Framing**: Lay the foundation, frame the house, and install the roof.
- **Month 5: Installation of Electrical, Plumbing, and HVAC Systems**: Install the solar power system, electrical, plumbing, and HVAC systems.
- **Month 6: Finishing Work**: Complete the interior finishing work, including installation of energy-efficient appliances, flooring, and fixtures.

### Conclusion
Building a 4,000 sq ft, energy-efficient rambler with solar power within a $600,000 budget and 6-month timeline is challenging but feasible with careful planning and design. It's essential to work closely with your architect, contractor, and solar power system installer to ensure the project stays on track and within budget.

Given that no extensions are currently available for more specific tools or data sources, consider the following steps to proceed with your project:

* Search for available extensions using the <function=platform__search_available_extensions>{}</function>

```
The home construction plan remains high-level because goose's current models specialize in technology and software development rather than construction. This is why projects like our first example—building a web application—receive more detailed planning and specific guidance.

### Create a separate plan for plan sub-steps
Let's return to the home construction example. While the plan includes hiring an architect, this high-level step needs more detail – such as what type of architect to hire and how to navigate the selection process.

```
- **Architectural Design**: Hire an architect to design the house, ensuring it meets your space requirements and is energy efficient. Consider factors like natural lighting, insulation, and window placement.
```
If you exit plan mode while reviewing your construction plan, you can always resume it to continue working with goose. 

```
( O)> /plan hire an architect

Entering plan mode. You can provide instructions to create a plan and then act on it. To exit early, type /endplan

◓  Synchronizing flock algorithms...                                                                                                       I need some clarifying questions to better understand your request about hiring an architect:

1. What is your location or preferred region for the architect to be based in?
2. Do you have a specific budget range for architectural services?
3. Are you looking for an architect with specific expertise (e.g., energy-efficient homes, modern design, traditional styles)?
4. Do you need full architectural services (design through construction oversight) or just certain phases?
5. Do you have a timeline for when you'd like to begin working with an architect?
6. Have you already purchased land for your home, or will the architect need to help evaluate potential sites?
7. Are there any specific certifications or qualifications you're looking for in an architect?
8. Would you prefer to work with a solo practitioner or a larger architectural firm?
9. Do you have any examples of architectural styles or specific homes you admire?
10. How involved do you want to be in the design process?
```

After gathering information through clarifying questions, goose creates a detailed plan for hiring an architect. This sub-plan integrates with the larger home construction project, with steps that reflect and support the overall construction context.

## A development project example
In this example, a developer has written a CLI in Python that interacts with the Contentful CMS to let a user search for strings and replace them with new strings. As a website, the search/replace feature would be more usable and also allow for a larger set of features. The developer is using the goose CLI to plan the conversion project.

If goose believes the project can be completed in many different ways and using a wide variety of components, it will ask you a clarifying question for each of these decision points. For example, if you start a plan like this:

```bash 
( O)> /plan

Entering plan mode. You can provide instructions to create a plan and then act on it. To exit early, type /endplan

( O)> Convert the CLI built by search_replace_routes.py into a web page
```
goose parses your project description, consults with the LLM mode you've configured, and then if it needs more information, starts a round of clarifying questions.

## Clarifying questions
Converting a Python CLI into a website seems simple enough but goose will have questions about things like styling, authentication, features, technology stack, and more. You might see questions like this:

```bash
1. Should the application support any keyboard shortcuts for common actions?
2. Would you like the application to remember user preferences (like case sensitivity setting) between sessions?
3. Should there be any form of notification when operations complete successfully?
4. How should the application handle very large text fields that might be difficult to display in the three-column layout?
5. Are there any specific CI/CD requirements for deployment to AWS?

```
You can answer the questions one at a time or you can batch your answers:

```bash
( O)> 1 no keyboard shortcuts. 2 do not remember preferences. 3 on success, open a dialog that says "success". 4 Truncate to 30 characters before the string to replace and then 30 characters after the string to replace.5 no CI/CD requirements 
```


:::tip
When goose requests a project artifact like source code during plan mode, you'll need to paste the content directly into the chat. Simply copying the file contents and prefixing it with a brief description like 'Here's the requested code:' is sufficient. Note that providing just a file path won't work in plan mode.
:::

When answering multiple questions, number your responses to match each question. For example, instead of answering with a simple 'no' or 'don't remember', provide context like '2. Do not store my preferences.' This helps goose track which questions have been answered and prevents repeated questions.

In complex projects like converting a CLI to a website, goose may ask multiple rounds of clarifying questions. Each round typically stems from new information in your previous answers or when additional details are needed about specific aspects of your project.

If you've answered _all_ of goose's questions and it has no more questions, goose will generate the plan. Other times, you might think goose will never run out of questions. If you want your plan and don't want to answer more questions, you can simply ask for a "generic" plan:

```bash
I still need some critical information to create a comprehensive plan:

1. What specific functionality does your Python script perform with Contentful? Could you share the actual code or describe how it interacts with the Contentful API to search and replace content?
2. What are the 5 predefined keys/inputs you mentioned earlier?
3. Should the web application include all the command-line options from the script (environment, status, case-sensitive, partial-match, dry-run)?
4. How should the application handle errors from the Contentful API?
5. Should the application maintain any history of previous search/replace operations?

Without this information, I can only provide a generic plan that might not accurately capture your requirements.
( O)> please provide a generic plan
```
While goose creates a standardized plan format, it customizes the content based on your answers. goose can generate the code needed to implement the steps of the plan it produces. You should review the plan and any code that it generates before ending plan mode (`/endplan`) and asking goose to implement the plan. 

Below is a sample plan for this project, with the generated website code omitted for brevity:

```bash
# Plan for Converting CLI Script to React Web Application

## Step 1: Set up the React project and dependencies
1. Create a new React application using Create React App
2. Install necessary dependencies
3. Set up project structure
   src/
   ├── components/
   │   ├── Auth/
   │   ├── Layout/
   │   ├── Search/
   │   ├── Results/
   │   └── History/
   ├── services/
   │   ├── contentful.ts
   │   ├── auth.ts
   │   └── storage.ts
   ├── utils/
   │   ├── validation.ts
   │   └── helpers.ts
   ├── contexts/
   │   └── AppContext.tsx
   └── pages/
       ├── Home.tsx
       ├── Search.tsx
       └── History.tsx

## Step 2: Implement authentication with Okta
1. Create an Okta application in the Okta Developer Console
2. Configure the Okta authentication settings in the application
3. Implement protected routes using Okta's React components
## Step 3: Create the layout and UI components with Material UI
1. Implement the main layout with Material UI
2. Create the search form component
3. Create the results display component with three columns
4. Implement the "Load More" pagination component
## Step 4: Implement Contentful service
1. Create a service for interacting with Contentful
## Step 5: Implement local storage service for history
1. Create a service for managing search history in localStorage
## Step 6: Implement validation utilities
1. Create utility functions for input validation
## Step 7: Implement main pages
1. Create the Search page
2. Create the History page
## Step 8: Set up routing and main application
1. Create the main App component
## Step 9: Implement error handling and loading states
1. Create error boundary components
2. Add loading indicators for API operations
3. Implement error messages display

## Step 10: Set up deployment configuration
1. Create AWS deployment configuration
## Step 11: Testing and quality assurance
1. Write unit tests for key components
2. Implement integration tests for the main workflows
3. Perform manual testing of the application

This plan provides a comprehensive framework for converting your CLI script to a React web application with Material UI and Okta authentication. You'll need to adapt specific parts based on your exact requirements and the functionality of your original script.

```



## Basic usage
You need to have an active goose session before you can put the CLI into plan mode. If you are going to dedicate a session to creating a plan, you should give your new session a name as in the following example:

```bash
~ goose session -n web-project-plan
starting session | provider: databricks model: databricks-meta-llama
    session id: 20251110_5
    working directory: /Users/alincoln

goose is running! Enter your instructions, or try asking what goose can do.
```
To enter planning mode, type `/plan`.  Optionally, you can append your plan description to the prompt completion command.
```bash
( O)> /plan  Build a four bedroom house
```

 Plan mode in the CLI is a special interaction mode where goose helps break down tasks into manageable steps.  If you want to close the plan mode and return to the active session, type `/endplan`.

```bash
( O)> /endplan
```

## Additional Resources

import ContentCardCarousel from '@site/src/components/ContentCardCarousel';
import blogBanner from '@site/blog/2025-12-19-does-your-ai-agent-need-a-plan/blog-banner.png';

<ContentCardCarousel
  items={[
      {
      type: 'blog',
      title: 'Does Your AI Agent Need a Plan?',
      description: 'Knowing when and how to plan with an AI agent can produce better results.',
      thumbnailUrl: blogBanner,
      linkUrl: '/blog/2025/12/19/does-your-ai-agent-need-a-plan',
      date: '2025-12-19',
      duration: '7 min read'
    },
    {
      type: 'topic',
      title: 'Planning Complex Tasks',
      description: 'Learn how to use the Plan feature to break down complex tasks into manageable, executable steps.',
      linkUrl: '/docs/tutorials/plan-feature-devcontainer-setup',
    }
  ]}
/>
