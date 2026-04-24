---
title: "Planning Complex Tasks"
description: "Learn how to use goose's Plan feature to break down complex tasks into manageable, executable steps."
---

# Planning Complex Tasks

*Transform overwhelming tasks into systematic, step-by-step execution plans*


Using goose for large, complex tasks can feel overwhelming, especially when you're unsure of exactly how you want to approach it in advance. I experienced this when I needed to set up a complex development environment for an [API course](https://github.com/LinkedInLearning/java-automated-api-testing-with-rest-assured-5989068) I published. Between Docker configurations, database initialization, devcontainer setup, and GitHub Codespaces integration, there are dozens of moving pieces that need to work together perfectly. One missing configuration or incorrect dependency can derail the entire process.

This tutorial shows you how to use goose's [Plan feature](/docs/guides/creating-plans) to transform a complex devcontainer setup into a systematic, executable roadmap. You'll learn how to brainstorm with goose, refine your requirements, and let goose create both a detailed plan and implementation checklist.

## What You'll Learn

By the end of this tutorial, you'll understand how to:

- Use goose's Plan feature to break down complex technical projects
- Brainstorm and refine requirements through AI-assisted conversation
- Transform high-level goals into detailed, executable steps
- Create comprehensive checklists for validation and quality assurance
- Execute plans systematically with clear deliverables

## The Challenge: Complete Codespaces Environment

Our goal is to create a GitHub repository that provides a complete development environment for API testing with Rest Assured. When students open the repository in GitHub Codespaces, they should have:

- **Automated Setup**: Zero manual configuration required
- **API Server**: REST API running automatically
- **Database**: MySQL with pre-loaded schema and test data
- **Development Tools**: Java 17, Docker, and necessary VS Code extensions
- **One-Click Experience**: Everything starts with `docker-compose up -d`

This involves coordinating multiple technologies: Docker, MySQL, PHP, Java, devcontainers, and GitHub Codespaces.


## Step 1: Initial Brainstorming Session

Let's start by having a brainstorming conversation with goose to clarify our requirements.

:::info Planning in Desktop vs CLI
The goose Desktop doesn't have a `/plan` keyword like the CLI. In Desktop, you need to explicitly ask goose to create a plan:

```
"Hey goose, can you create a plan to set up my GitHub repo for Codespaces with automatic Docker startup? Please don't start the actual work yet."
```

In the CLI, you can enter plan mode with `/plan` and goose will ask clarifying questions before generating the plan. Unless you specifically ask goose Desktop to "create a plan", it might jump straight into implementation.
:::

### Starting the Conversation

**You:** _"I need help setting up a GitHub repository for automated API testing. Students should be able to open it in GitHub Codespaces and immediately start writing tests without any manual setup. Can you help me think through what's needed?"_

**goose:** _"I'd be happy to help you design this automated setup! Let me ask some clarifying questions to understand your requirements better:_

1. What type of API are you working with? (REST, GraphQL, etc.)
2. What testing framework do you want students to use?
3. Do you need a real database with test data, or can you use mocked responses?
4. What programming language should students use for their tests?
5. Are there specific API endpoints or functionality they'll be testing?"

### Refining Requirements

**You:** _"It's a REST API built in PHP that students will test using Rest Assured in Java. They need a real MySQL database with actual test data so they can practice realistic scenarios. The API has endpoints for managing products and categories in an e-commerce system."_

**goose:** _"Perfect! That gives me a clear picture. A few more questions to ensure we design this correctly:_

1. Do you already have the PHP API code, or do we need to create it?
2. What version of Java should the environment support?
3. Should the MySQL database be populated with sample products and categories?
4. Do you want any specific VS Code extensions pre-installed?
5. What's your timeline for getting this working?"

**You:** _"I have the existing PHP API code that currently runs in MAMP. I need Java 17 for Rest Assured compatibility. Yes, the database should have sample data. Please include Java and Docker extensions for VS Code."_

## Step 2: Creating the Detailed Plan

Now that we've clarified the requirements, let's ask goose to create a comprehensive plan.

### Requesting the Plan

**You:** _"Based on our discussion, can you create a detailed plan for setting up this GitHub repository? I want a step-by-step approach that covers all the technical requirements we discussed. Please include specific deliverables and a way to validate each step."_

### The Generated Plan

goose will generate a comprehensive plan similar to this:

<details>
  <summary>GitHub Codespaces API Testing Environment Setup Plan</summary>

```markdown
# GitHub Codespaces API Testing Environment Setup Plan

## Project Overview
Create a GitHub repository that automatically provisions a complete development environment for API testing with Rest Assured, including PHP API server, MySQL database, and Java development tools.

## Phase 1: Repository Structure and API Integration

### 1.1 Handle the API Files
- **Deliverable**: Organized API codebase in repository
- **Tasks**:
  - Place `/api-server/` directory at repository root
  - Organize PHP files into logical structure:
    
    /api-server/
        category/
        config/
        objects/
        product/
        ApiTestingDB.sql
    

- **Validation**: All PHP files accessible and properly organized

### 1.2 Database Schema and Test Data
- **Deliverable**: SQL file with schema and sample data
- **Tasks**:
  - Review existing `ApiTestingDB.sql` file
  - Ensure it includes both schema creation and sample data insertion
  - Verify sample data covers realistic e-commerce scenarios
- **Validation**: SQL file executes without errors and populates test data

## Phase 2: Containerization

### 2.1 Dockerize the API Server
- **Deliverable**: `api-server/Dockerfile`
- **Tasks**:
  - Create Dockerfile using `php:8.1-apache` base image
  - Install required PHP extensions (`pdo_mysql`)
  - Configure proper file permissions and Apache settings

- **Example Configuration**:
  
  FROM php:8.1-apache
  RUN docker-php-ext-install pdo pdo_mysql
  COPY . /var/www/html/
  
- **Validation**: Docker image builds successfully and serves PHP files

### 2.2 Database Container Setup
- **Deliverable**: MySQL service configuration in docker-compose.yml
- **Tasks**:
  - Configure MySQL 5.7 container with proper environment variables
  - Set up automatic schema initialization using volume mounting
  - Configure networking for PHP-MySQL communication
- **Database Credentials**:
  | Setting  | Value |
  |----------|-------|
  | Host     | `mysql` |
  | Database | `api_db` |
  | Username | `user` |
  | Password | `password` |

- **Validation**: Database starts automatically and initializes with test data

### 2.3 Service Orchestration
- **Deliverable**: Complete `docker-compose.yml`
- **Tasks**:
  - Define both API server and MySQL services
  - Configure proper service dependencies
  - Set up port mapping for external access
  - Configure volume mounting for database initialization
- **Example Configuration**:
  
  version: '3.8'
  services:
    mysql:
      image: mysql:5.7
      restart: always
      environment:
        MYSQL_ROOT_PASSWORD: password
        MYSQL_DATABASE: api_db
        MYSQL_USER: user
        MYSQL_PASSWORD: password
      ports:
        - "3306:3306"
      volumes:
        - ./api-server/sql:/docker-entrypoint-initdb.d
    
    api-server:
      build: ./api-server
      ports:
        - "8888:80"
      depends_on:
        - mysql
  
- **Validation**: Both services start correctly and can communicate

## Phase 3: Database Connection Configuration

### 3.1 Update PHP Database Configuration
- **Deliverable**: Updated `api-server/config/database.php`
- **Tasks**:
  - Modify connection settings to use Docker networking
  - Update host from `localhost` to `mysql`
  - Verify credentials match docker-compose environment variables
- **Validation**: PHP API can successfully connect to MySQL container

## Phase 4: Codespaces Integration

### 4.1 Devcontainer Configuration
- **Deliverable**: `.devcontainer/devcontainer.json`
- **Tasks**:
  - Create devcontainer configuration for Java 17 development
  - Enable Docker-in-Docker functionality
  - Configure automatic container startup
  - Pre-install necessary VS Code extensions
- **Example Configuration**:
  
  {
    "name": "API Testing Environment",
    "image": "mcr.microsoft.com/devcontainers/java:17",
    "features": {
      "docker-in-docker": "latest"
    },
    "postCreateCommand": "docker-compose up -d",
    "customizations": {
      "vscode": {
        "extensions": [
          "vscjava.vscode-java-pack",
          "ms-azuretools.vscode-docker"
        ]
      }
    }
  }
  
- **Validation**: Codespace launches with all tools available

## Phase 5: Testing and Validation

### 5.1 API Access Verification
- **Tasks**:
  - Verify API accessible at `http://localhost:8888`
  - Test key endpoints (products, categories)
  - Confirm database queries return expected data
- **Validation**: All API endpoints respond correctly

### 5.2 Codespaces Integration Testing
- **Tasks**:
  - Open repository in GitHub Codespaces
  - Verify automatic Docker startup
  - Test VS Code extensions functionality
  - Confirm Java 17 availability
- **Validation**: Complete environment ready for student use

## Quality Assurance Checklist

### ✅ API Server and Database Setup
- [ ] `/api-server/` placed at repository root
- [ ] All required directories present (category/, config/, objects/, product/)
- [ ] `ApiTestingDB.sql` includes schema and test data
- [ ] `Dockerfile` uses `php:8.1-apache` with `pdo_mysql` extension
- [ ] `docker-compose.yml` defines both MySQL and API services
- [ ] Services configured with correct dependencies and port mapping

### ✅ Database Configuration
- [ ] PHP database config uses Docker networking (`mysql` host)
- [ ] Credentials match between PHP config and docker-compose
- [ ] Database initializes automatically with test data

### ✅ Codespaces Configuration
- [ ] `.devcontainer/` folder exists at repository root
- [ ] `devcontainer.json` specifies Java 17 environment
- [ ] Docker-in-Docker feature enabled
- [ ] `postCreateCommand` set to `docker-compose up -d`
- [ ] Java and Docker VS Code extensions configured

### ✅ Integration Testing
- [ ] Codespace launches without errors
- [ ] Docker services start automatically
- [ ] API accessible at `http://localhost:8888`
- [ ] Database queries return expected test data
- [ ] Java 17 available for Rest Assured development

## Risk Assessment and Mitigation

### Potential Issues
1. **Port Conflicts**: Codespaces might have port conflicts
   - *Mitigation*: Use non-standard ports (8888 for API, 3307 for MySQL)
2. **Docker Startup Timing**: API might start before database is ready
   - *Mitigation*: Implement proper health checks and startup delays
3. **File Permissions**: Docker might have permission issues with mounted volumes
   - *Mitigation*: Set appropriate file permissions in Dockerfile

### Success Criteria
- Students can open Codespace and immediately start writing tests
- No manual configuration or setup steps required
- All API endpoints accessible and functional
- Database populated with realistic test data
```

</details>

## Step 3: Detailed Implementation Planning

Now let's ask goose to create the specific implementation prompt that will guide the execution.

### Creating the Implementation Prompt

**You:**  _"This plan looks comprehensive! Can you now create a detailed implementation prompt that I can use to execute this plan? I want something specific enough that goose can follow it step-by-step without needing additional clarification."_

### The Implementation Prompt

goose will generate a detailed prompt like this:

<details>
    <summary>Detailed Prompt</summary>

```markdown
**Task:**
Set up my GitHub repo for GitHub Codespaces with automatic Docker startup for the local API server.

**Instructions:**

1. Create a .devcontainer folder in the root of the repo.
2. Inside .devcontainer, create a devcontainer.json file with the following:
  - Use a Java 17 development image: "image": "mcr.microsoft.com/devcontainers/java:17"
  - Add "features": { "docker-in-docker": "latest" }
  - Set "postCreateCommand": "docker-compose up -d" to automatically start the Docker containers after the Codespace builds.
  - Optionally install VS Code extensions for Java and Docker support ("vscode-java-pack" and "vscode-docker").
3. Create a docker-compose.yml file at the root of the repo.
  - Define the service that runs the API server.
  - Expose necessary ports (e.g., if the API runs on localhost:8888, map it accordingly).

    Example service config:

    `
    version: '3.8'
    services:
      api-server:
        build: ./api-server
        ports:
          - "8888:80"
        volumes:
          - ./api-server:/var/www/html
        depends_on:
          - database (if needed)
    

  - Adjust the build context and ports based on the actual API being used.

4. If needed, create a Dockerfile under ./api-server/ to define how the API server should be built (especially if this was originally a PHP app in MAMP).
5. Ensure that once a Codespace launches:
  - The API server is already running
  - Users can immediately start writing and running Java tests against the API without any additional setup.
6. Confirm that Docker is working inside the Codespace by using "docker ps" and hitting the API URL in a test after startup.

**Assumptions:**

  - The API server is already included in the repo or will be added.
  - If a database is required (e.g., MySQL for the API), add it as a separate container in docker-compose.yml and configure as needed.

**Deliverables:**

- .devcontainer/devcontainer.json
- docker-compose.yml
- (optional) api-server/Dockerfile
- Confirmed Codespace readiness.
```

</details>

## Step 4: Executing the Plan

With our detailed plan and implementation prompt ready, we can now execute the setup systematically.

### Starting Implementation

**You:** _"Perfect! Now let's implement this plan. Here's the detailed prompt we created: [paste the implementation prompt]"_

goose will now work through each step of the plan, creating the necessary files and configurations.

### Monitoring Progress

As goose implements the plan, you can verify each deliverable against the checklist we created:

1. **File Creation**: Check that each required file is created in the correct location
2. **Configuration Accuracy**: Verify that configurations match the specifications
3. **Integration Points**: Ensure services are properly connected and configured
4. **Testing Readiness**: Confirm that the environment will work as intended

### Validation and Testing

Once implementation is complete, follow the validation steps from the plan:

```bash
# Test Docker setup locally
docker-compose up -d
docker ps

# Verify API accessibility
curl http://localhost:8888/api/products

# Test database connection
docker exec -it mysql mysql -u user -p api_db
```

## Step 5: Iterative Refinement

If issues arise during implementation or testing, use the plan as a reference to identify and resolve problems systematically.

### Common Refinements

- **Port Conflicts**: Adjust port mappings if conflicts occur
- **Timing Issues**: Add health checks or startup delays
- **Permission Problems**: Update Dockerfile with proper permissions
- **Configuration Errors**: Verify environment variables and connection strings

## Best Practices for Planning with goose

1. Start with brainstorming. Don't jump straight to asking for a plan. Have a conversation to clarify requirements and explore options.
2. Be specific about deliverables. Ask for concrete deliverables, file names, and validation criteria for each step.
3. Ask goose to identify potential issues and provide mitigation strategies.
4. Generate detailed implementation prompts that can guide execution without additional clarification.
5. Include specific ways to verify that each step worked correctly.

## Advanced Planning Techniques

### Multi-Phase Planning
For very complex projects, break the plan into phases and tackle them incrementally:

**You:** _"This plan is quite comprehensive. Can you break it into smaller phases that I can implement and test independently?"_

### Dependency Mapping
Ask goose to identify dependencies between tasks:

**You:** _"Which of these tasks can be done in parallel, and which have dependencies on other tasks?"_

### Alternative Approaches
Explore different implementation strategies:

**You:** _"Are there alternative approaches to this setup that might be simpler or more robust?"_

## Conclusion

goose's Plan feature transforms complex technical challenges from overwhelming problems into systematic, executable roadmaps. By following this tutorial's approach:

1. **Brainstorm** to clarify requirements and explore options
2. **Plan** to break down complexity into manageable steps
3. **Detail** to create specific implementation guidance
4. **Execute** systematically with clear validation criteria
5. **Refine** iteratively based on results and feedback

You can tackle any complex development environment setup with confidence, knowing that you have a clear path forward and specific criteria for success.

The key is treating goose as a planning partner, not just a code generator. Give it the full context of what you're trying to achieve, and let it help you think through the complexity before diving into implementation.

## Next Steps

- Try this approach with your own complex setup challenges
- Experiment with different types of planning prompts
- Share your planning successes with the [goose community](https://discord.gg/goose-oss)
- Explore how planning integrates with [Subagents](/docs/guides/subagents) for even more sophisticated workflows

Remember, the goal is to get the right approach, in the right order, with the right safeguards. That's what makes the difference between a quick fix and a robust, maintainable solution.