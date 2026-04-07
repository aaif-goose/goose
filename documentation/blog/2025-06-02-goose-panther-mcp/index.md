---
title: "Democratizing Detection Engineering at Block: Taking Flight with Goose and Panther MCP"
description: "A comprehensive overview of how Block leverages Goose and Panther MCP to democratize and accelerate security detection engineering."
authors:
  - tomasz
  - glenn
---

![blog cover](goose-panther-header.png)

Detection engineering stands at the forefront of cybersecurity, yet it’s often a tangled web of complexity. Traditional detection writing involves painstaking manual processes encompassing log format and schema comprehension, intricate query creation, threat modeling, and iterative manual detection testing and refinement, leading to time expenditure and reliance on specialized expertise. This can lead to gaps in threat coverage and an overwhelming number of alerts. At Block, we face the relentless challenge of evolving threats and intricate system complexities. To stay ahead, we've embraced AI-driven solutions, notably Goose, Block’s open-source AI agent, and Panther MCP, to allow the broader organization to contribute high-quality rules that are contextual to their area of expertise. This post delves into how we're transforming complicated detection workflows into streamlined, AI-powered, accessible processes for all stakeholders.

<!-- truncate -->

## The Detection Engineering Challenge

Historically, creating effective detections has been a niche skill, requiring deep technical knowledge and coding proficiency. This has created significant obstacles such as:

* **Steep Learning Curve:** Crafting detections typically requires extensive technical expertise, often limiting participation.  
* **Resources Constraints:** Even expert security teams often struggle with bandwidth, hindering their ability to develop and deploy new detections quickly.  
* **Evolving Threat Landscape:** Advanced threats, particularly those from sophisticated nation-states actors, continuously evolve, outpacing traditional detection development processes.

## Vision

We envision a future where anyone at Block can effortlessly create and deploy security detections, revolutionizing our defenses through intelligent automation and empowering a democratized security posture.

## Introducing Panther MCP

### What is Panther MCP?

[Panther MCP](https://github.com/panther-labs/mcp-panther) is an open-source model context protocol server born from the collaboration between [Panther](https://panther.com/) and Block to democratize security operations workflows. By tightly integrating with Goose as an extension, Panther MCP allows security teams at Block to translate natural language instructions into precise, executable SIEM detection logic, making threat detection contributions easier and faster than ever.

This integration empowers analysts and engineers across Block to interact with Panther’s security analytics platform seamlessly. It shifts detection development from a coding-heavy process into an intuitive workflow accessible to everyone, regardless of technical background. Goose serves as an intermediary agent, coordinating calls to Panther MCP, reviewing the output, creating rule content, testing it, and making necessary edits for correctness or style. This AI-driven feedback loop saves countless hours of time.

### Key Features

Panther MCP offers dozens of tools that enhance and accelerate detection engineering workflows powered by Goose:

1. **Natural Language to Detection Logic**  
   Engineers define detections using plain English prompts, which Panther MCP translates directly into Panther-compatible detection rules that can be checked into their [panther-analysis](https://github.com/panther-labs/panther-analysis) repository.  
2. **Interactive Data Exploration and Usage**  
   Engineers can rapidly explore log sources and perform searches on data and previously generated alerts through quick, natural-language driven interactions.  
3. **Unified Alert Triage and Response**  
   Enables AI-led alert triage with insights drawn from historical data and existing detections.

## Accelerating Detection Creation with Goose

Goose significantly accelerates security detection creation by using AI to automate traditionally manual tasks like log analysis and rule generation. This drastically reduces effort, improves the speed of developing and deploying threat coverage, and enhances agility against evolving threats.

### Integrating Panther MCP as a Goose Extension

Panther MCP functions as a Goose extension, seamlessly embedding its capabilities within the Goose environment through the following process:

1. **Extension Registration:** Panther MCP is registered within Goose, making its suite of tools readily accessible via the Goose interface.  
2. **API Connectivity:** The extension establishes a connection to Panther's backend API, enabling seamless context retrieval.  
3. **Available Tools:** Panther MCP provides Goose with a range of tools designed for efficient detection creation, intuitive data interaction, and streamlined alert management.

### Leveraging Enhanced Context with `.goosehints`

The integration between Panther MCP and Goose is enhanced through the use of the [.goosehints](/docs/guides/context-engineering/using-goosehints/) file—a Goose feature that supplies additional context like rule examples and best practices. This enriched context enables Goose to generate more accurate and efficient detections, aligned with Block’s standards and requirements. 

Let's illustrate this with an example: creating a rule to detect users adding themselves to privileged Okta groups, a common privilege escalation technique.

## Breaking Down the Barriers

Traditionally, creating this detection would require:

1. Deep knowledge of Okta and its log structure  
2. Understanding of Panther’s detection framework  
3. Python programming skills  
4. Familiarity with different testing frameworks

With Goose and Panther MCP, this becomes as simple as:

> “Write a detection rule for users adding themselves to privileged Okta groups.”

## The Intelligence Behind the Simplicity

When a natural language request like "Write a detection rule for users adding themselves to privileged Okta groups" is received, Goose leverages a sophisticated, multi-stage process powered by Panther MCP to generate production-ready detection logic. This automated approach mirrors the workflow of an experienced detection engineer, encompassing threat research, relevant log identification, detection goal definition, logic outlining, sample log analysis, rule development, false positive consideration, severity/context assignment, thorough testing, refinement/optimization, and documentation. However, Goose executes these steps with the speed and scalability afforded by AI and automation.

Goose first parses the natural language input to understand the core intent and requirements. It identifies key entities like "users", "privileged Okta groups", and the action "adding themselves". This understanding forms the basis for outlining the detection's objective, the necessary log source (`Okta.SystemLog`), and the fundamental logic: identifying events where the actor (user initiating the action) is the same as the target user (the user being added to the group), and the group being joined is designated as privileged. Goose also considers potential false positives (e.g., legitimate automated processes) and assigns a preliminary severity level based on the potential impact of the detected activity (privilege escalation).

![Process overview diagram](process-overview-diagram.png)

To ensure the generated logic is accurate and operates on valid data, Goose interacts with Panther MCP to retrieve the schema of the specified log source (`Okta.SystemLog`). This provides Goose with a structured understanding of the available fields and their data types within Okta logs. Furthermore, Goose utilizes Panther MCP's querying capabilities to fetch sample log events related to group membership changes. This step is crucial for:

* **Identifying Common Event Patterns:** Analyzing real-world logs allows Goose to understand the typical structure and values associated with relevant events (e.g., `group.user_membership.add`).  
* **Inferring Privileged Group Naming Conventions:** By examining historical data, Goose can identify patterns and keywords commonly used in the naming of privileged groups within the organization's Okta instance (e.g., "admin", "administrator", "security-admin").  
* **Discovering Edge Cases:** Examining diverse log samples helps uncover potential variations in event data or less common scenarios that the detection logic needs to accommodate.  
* **Mapping Typical User Behavior:** Understanding baseline user behavior around group membership changes helps refine the detection logic and reduce the likelihood of false positives.

The interaction with Panther MCP at this stage involves API calls to retrieve schema information and execute analytical queries, enabling Goose to ground its reasoning in actual log data.

![Goose interacts with Panther MCP](goose-panther-mcp-interaction.png)

Goose doesn't operate in isolation; it accesses a repository of existing Panther detection rules to identify similar logic or reusable components. This promotes consistency across the detection landscape, encourages the reuse of well-tested helper functions (like `okta_alert_context`), and ensures adherence to established rule standards within our security ecosystem. Learning from existing detections is a core component of Goose’s intelligence, allowing it to build upon prior knowledge and avoid reinventing the wheel.

![Rule context reuse](context-reuse-example.png)

Based on the understanding of the detection goal, the analysis of log data, and the knowledge gleaned from existing detections facilitated by Panther MCP, Goose generates the complete Panther detection rule in Python. This includes:

* **Rule Function (`rule()`):** This function contains the core logic for evaluating each log event. In the example, it checks for the `group.user_membership.add` event type, verifies that the actor and target user IDs (or emails) are the same, and confirms that the target group's display name contains keywords indicative of a privileged group (defined in the `PRIVILEGED_GROUPS` set).  
* **Metadata Functions (`title()`, `alert_context()`, `severity()`, `destinations()`):** These functions provide crucial context and operational information for triggered alerts.

```python
from panther_okta_helpers import okta_alert_context

# Define privileged Okta groups - customize this list based on your organization's needs
PRIVILEGED_GROUPS = {
    "_group_admin",  # Administrator roles
    "admin",
    "administrator",
    "application-admin",   
    "aws_",  # AWS roles can be privileged
    "cicd_corp_system",  # CI/CD admin access 
    "grc-okta",
    "okta-administrators",
    "okta_admin",
    "okta_admin_svc_accounts", # Admin roles
    "okta_resource-set_",      # Resource sets are typically privileged
    "security-admin",
    "superadministrators",
}

def rule(event):
    """Determine if a user added themselves to a privileged group"""
    # Only focus on group membership addition events
    if event.get("eventType") != "group.user_membership.add":
        return False
    # Ensure both actor and target exist in the event
    actor = event.get("actor", {})
    targets = event.get("target", [])
    if not actor or len(targets) < 2:
        return False
    actor_id = actor.get("alternateId", "").lower()
    actor_user_id = actor.get("id")
    # Extract target user and group
    target_user = targets[0]
    target_group = targets[1] if len(targets) > 1 else {}
    # The first target should be a user and the second should be a group
    if target_user.get("type") != "User" or target_group.get("type") != "UserGroup":
        return False
    target_user_id = target_user.get("id")
    target_user_email = target_user.get("alternateId", "").lower()
    group_name = target_group.get("displayName", "").lower()
    # Check if the actor added themselves to the group
    is_self_add = (actor_user_id == target_user_id) or (actor_id == target_user_email)
    # Check if the group is privileged
    is_privileged_group = any(priv_group in group_name for priv_group in PRIVILEGED_GROUPS)
    return is_self_add and is_privileged_group

def title(event):
    """Generate a descriptive title for the alert"""
    actor = event.get("actor", {})
    targets = event.get("target", [])
    actor_name = actor.get("displayName", "Unknown User")
    actor_email = actor.get("alternateId", "unknown@example.com")
    target_group = targets[1] if len(targets) > 1 else {}
    group_name = target_group.get("displayName", "Unknown Group")
    return (f"User [{actor_name} ({actor_email})] added themselves "
            f"to privileged Okta group [{group_name}]")

def alert_context(event):
    """Return additional context for the alert"""
    context = okta_alert_context(event)
    # Add specific information about the privileged group
    targets = event.get("target", [])
    if len(targets) > 1:
        target_group = targets[1]
        context["privileged_group"] = {
            "id": target_group.get("id", ""),
            "name": target_group.get("displayName", ""),
        }
    return context

def severity(event):
    """Calculate severity based on group name - more sensitive groups get higher severity"""
    targets = event.get("target", [])
    if len(targets) <= 1:
        return "Medium"
    target_group = targets[1]
    group_name = target_group.get("displayName", "").lower()
    # Higher severity for direct admin groups
    if any(name in group_name for name in ["admin", "administrator", "superadministrators"]):
        return "Critical"
    return "High"

def destinations(_event):
    """Send to staging destination for review"""
    return ["staging_destination"]
```

Beyond the Python code, Goose also generates the corresponding YAML-based rule configuration file. This file contains essential metadata about the detection:

```yaml
AnalysisType: rule
Description: Detects when a user adds themselves to a privileged Okta group, which could indicate privilege escalation attempts or unauthorized access.
DisplayName: "Users Adding Themselves to Privileged Okta Groups"
Enabled: true
DedupPeriodMinutes: 60
LogTypes:
  - Okta.SystemLog
RuleID: "goose.Okta.Self.Privileged.Group.Add"
Threshold: 1
Filename: goose_okta_self_privileged_group_add.py
Reference: >
  https://developer.okta.com/docs/reference/api/system-log/
  https://attack.mitre.org/techniques/T1078/004/
  https://attack.mitre.org/techniques/T1484/001/
Runbook: >
  1. Verify if the user should have access to the privileged group they added themselves to
  2. If unauthorized, revoke the group membership immediately
  3. Check for other group membership changes made by the same user
  4. Review the authentication context and security context for suspicious indicators
  5. Interview the user to determine intent
Reports:
  MITRE ATT&CK:
    - TA0004:T1078.004  # Privileged Accounts: Cloud Accounts
    - TA0004:T1484.001  # Domain Policy Modification: Group Policy Modification
Severity: High
Tags:
  - author:tomasz
  - coauthor:goose
Tests:
  - Name: User adds themselves to privileged group
    ExpectedResult: true
    Log:
      actor:
        alternateId: jane.doe@company.com
        displayName: Jane Doe
        id: 00u1234abcd5678
        type: User
      authenticationContext:
        authenticationStep: 0
        externalSessionId: xyz1234
      client:
        device: Computer
        geographicalContext:
          city: San Francisco
          country: United States
          geolocation:
            lat: 37.7749
            lon: -122.4194
          postalCode: "94105"
          state: California
        ipAddress: 192.168.1.100
        userAgent:
          browser: CHROME
          os: Mac OS X
          rawUserAgent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/108.0.0.0 Safari/537.36
        zone: "null"
      debugContext:
        debugData:
          requestId: req123456
          requestUri: /api/v1/groups/00g123456/users/00u1234abcd5678
          url: /api/v1/groups/00g123456/users/00u1234abcd5678
      displayMessage: Add user to group membership
      eventType: group.user_membership.add
      legacyEventType: group.user_membership.add
      outcome:
        result: SUCCESS
      published: "2023-07-15 14:25:30.811"
      request:
        ipChain:
          - geographicalContext:
              city: San Francisco
              country: United States
              geolocation:
                lat: 37.7749
                lon: -122.4194
              postalCode: "94105"
              state: California
            ip: 192.168.1.100
            version: V4
      securityContext:
        asNumber: 12345
        asOrg: Example ISP
        domain: example.com
        isProxy: false
        isp: Example ISP
      severity: INFO
      target:
        - alternateId: jane.doe@company.com
          displayName: Jane Doe
          id: 00u1234abcd5678
          type: User
        - alternateId: unknown
          displayName: okta_admin_person_role_super_admin
          id: 00g5678abcd1234
          type: UserGroup
      transaction:
        detail: {}
        id: transaction123
        type: WEB
      uuid: event-uuid-123
      version: "0"
      p_event_time: "2023-07-15 14:25:30.811"
      p_parse_time: "2023-07-15 14:26:00.000"
      p_log_type: "Okta.SystemLog"
  - Name: User adds another user to privileged group
    ExpectedResult: false
    Log:
      actor:
        alternateId: admin@company.com
        displayName: Admin User
        id: 00u5678abcd1234
        type: User
      authenticationContext:
        authenticationStep: 0
        externalSessionId: xyz5678
      client:
        device: Computer
        geographicalContext:
          city: San Francisco
          country: United States
          geolocation:
            lat: 37.7749
            lon: -122.4194
          postalCode: "94105"
          state: California
        ipAddress: 192.168.1.100
        userAgent:
          browser: CHROME
          os: Mac OS X
          rawUserAgent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/108.0.0.0 Safari/537.36
        zone: "null"
      debugContext:
        debugData:
          requestId: req789012
          requestUri: /api/v1/groups/00g123456/users/00u9876fedc4321
          url: /api/v1/groups/00g123456/users/00u9876fedc4321
      displayMessage: Add user to group membership
      eventType: group.user_membership.add
      legacyEventType: group.user_membership.add
      outcome:
        result: SUCCESS
      published: "2023-07-15 14:30:45.123"
      request:
        ipChain:
          - geographicalContext:
              city: San Francisco
              country: United States
              geolocation:
                lat: 37.7749
                lon: -122.4194
              postalCode: "94105"
              state: California
            ip: 192.168.1.100
            version: V4
      securityContext:
        asNumber: 12345
        asOrg: Example ISP
        domain: example.com
        isProxy: false
        isp: Example ISP
      severity: INFO
      target:
        - alternateId: user@company.com
          displayName: Regular User
          id: 00u9876fedc4321
          type: User
        - alternateId: unknown
          displayName: okta_admin_person_role_super_admin
          id: 00g5678abcd1234
          type: UserGroup
      transaction:
        detail: {}
        id: transaction456
        type: WEB
      uuid: event-uuid-456
      version: "0"
      p_event_time: "2023-07-15 14:30:45.123"
      p_parse_time: "2023-07-15 14:31:00.000"
      p_log_type: "Okta.SystemLog"
```

Every detection rule generated by Goose undergoes rigorous automated testing and validation. This includes:

* **Unit Testing:** Using the test cases defined in the rule configuration, the Panther Analysis Tool is executed to verify that the rule logic correctly identifies true positives and avoids false negatives against simulated log data.  
* **Linting:** Code linting tools (like Pylint) are automatically run to ensure the generated Python code adheres to established coding standards, including proper formatting, style conventions, and best practices. This contributes to code maintainability and reduces the risk of errors.

![Automated testing graphic](automated-testing-graphic.png)
![Process improvement chart](process-improvement-chart.png)

The seamless integration of Goose with Panther MCP automates these intricate steps, significantly reducing the time and specialized knowledge required to create and deploy security detections. This democratization empowers more individuals to contribute to Block's security posture, leading to more comprehensive threat coverage and a more resilient security environment.

## Democratization in Practice

A typical detection creation workflow now looks like:

1. **Proposal:** A user describes a malicious behavior in natural language.  
2. **Generation:** Goose transforms this description into detection logic with Panther MCP.  
3. **Review:** The detection team reviews each detection against defined quality benchmarks.  
4. **Deployment:** Approved detections are deployed to staging/production.

## Early Impact & Lessons Learned

### Expanding Collaboration to Enhance Coverage and Enable Self-Service

* **Lowering the Technical Barrier:** Goose and Panther MCP empower subject matter experts (SMEs) to easily understand their logs in Panther, enabling a self-service model where teams can create their own detections without extensive security engineering expertise, thus distributing the workload.  
* **Reduced Dependency on the Detection Team:** Panther MCP reduces security team dependency by enabling users to independently resolve inquiries autonomously. This includes threat intelligence teams assessing MITRE ATT&CK coverage, compliance teams identifying relevant detections, and helping service SMEs create their own detections.  
* **Cross-Functional Detection Development:** Democratizing detection engineering allows specialized teams to create detections that security teams might miss, leading to a more diverse detection ecosystem covering niche use cases. This fosters two-way knowledge transfer, enhancing overall security awareness and capabilities.

### Accelerating the Detection Development Lifecycle

* **Contextual Understanding:** Detection engineering is becoming more efficient and consistent through tools that embed organizational context, provide guided best practices, understand existing log schemas and detections, and align with validation frameworks such as *pytest*. This approach enables broader participation and supports high-quality development across teams.  
* **Streamlined Development Process:** Natural language interfaces are simplifying detection engineering by allowing users to interact with the system conversationally. This enables automated retrieval of example logs, analysis of log schemas, interpretation of detection goals or required changes, and generation of initial detection code—significantly accelerating development.  
* **Automated Technical Steps:** Intelligent code generation incorporates error handling and best practices, while seamlessly generating test cases from data and producing comprehensive documentation—including descriptions, runbooks, and references.

### Driving Consistency via Standardized Practices

* **Code Style and Structure:** Newly created detections adhere to consistent stylistic patterns, utilizing dedicated functions for specific checks instead of overloaded `rule()` checks. Standardized formatting, including brackets for dynamic alert title text, enhances readability and consistency.  
* **Code Reuse and Efficiency:** Promote code reuse and efficiency through global helpers/filters, explicit typing in function signatures, and detailed docstrings for better function understanding and LLM code generation.  
* **Maintainability Improvements:** Detections are designed with a consistent structure and standardized patterns, making them easier to understand, maintain, and update. This uniformity ensures predictable behavior across the detection code base and simplifies bulk changes when needed.  
* **Comprehensive Testing Requirements:** For our team, each detection is required to include at least two unit tests: one positive case that triggers the detection and one negative case that does not. Test names are descriptive and aligned with expected outcomes to enhance readability and maintainability.  
* **Metadata and Documentation Standards:** Metadata and documentation standards are being strengthened through structured definitions within pytests, helping to codify detection ownership and context. This includes clearly defined author and coauthor tags (e.g., for Goose-generated content), environment references such as staging or production, and accurate mapping of alert destinations.  
* **Structural Validation:** This supports compliance with organizational standards by enforcing filename conventions (e.g., prefixing, length, lowercase formatting), ensuring Python rules include all required functions, and verifying that YAML files contain the necessary fields for proper functionality and processing.  
* **Security Framework Alignment:** Relevant rules are mapped to applicable MITRE ATT&CK techniques to highlight coverage gaps, inform detection development, prioritize research efforts, and establish a common language for discussing threats.

### Best Practices and Safeguards

* **Platform-Conformant Development:** Detections are developed in alignment with Panther’s recommended practices, such as using built-in event object methods like `event.deep_get()` and `event.deep_walk()` instead of importing them manually, ensuring consistency and maintainability within the platform.  
* **Proactive Error Prevention:** We implement local validation checks through pre-commit and pre-push hooks to proactively catch and resolve errors before they reach upstream builds. These checks include validating alert destination names, verifying log types, and flagging grammatical issues to ensure quality and consistency.  
* **Continuous Improvement:** Detection quality continuously improves by incorporating feedback, performance data, and analysis of detection trends. Panther MCP, along with other ticket tracking MCPs, provides insights from analyst feedback and alert dispositions, which facilitates automated adjustments, streamlines pull request development, and lowers operational overhead.

## What’s Next?

Block is dedicated to improving its security defenses and supporting its team by leveraging AI. We believe AI holds significant promise for the future of detection and response at Block and are committed to making security more accessible.

<!-- Social Media Meta Tags (edit values as needed) -->
<head>
  <meta property="og:title" content="Democratizing Detection Engineering at Block: Taking Flight with Goose and Panther MCP" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/06/02/goose-panther-mcp" />
  <meta property="og:description" content="A comprehensive overview of how Block leverages Goose and Panther MCP to democratize and accelerate security detection engineering." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/goose-panther-header-25b5891acdd70e6a7bbe6b84e34f08f0.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="Democratizing Detection Engineering at Block: Taking Flight with Goose and Panther MCP" />
  <meta name="twitter:description" content="A comprehensive overview of how Block leverages Goose and Panther MCP to democratize and accelerate security detection engineering." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/goose-panther-header-25b5891acdd70e6a7bbe6b84e34f08f0.png" />
</head>
