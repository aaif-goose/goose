# 🍳 Contributing Recipes to goose Cookbook

Thank you for your interest in contributing to the goose Recipe Cookbook! This guide will walk you through the process of submitting your own recipe.

## 🚀 Quick Start

1. [Fork this repository](https://github.com/aaif-goose/goose/fork)
2. Add your recipe file here: `documentation/src/pages/recipes/data/recipes/`
3. Create a pull request

## 📋 Step-by-Step Guide

### Step 1: Fork the Repository

Click the **"Fork"** button at the top of this repository to create your own copy.

### Step 2: Create Your Recipe File

1. **Navigate to**: `documentation/src/pages/recipes/data/recipes/`
2. **Create a new file**: `your-recipe-name.yaml`
3. **Important**: Choose a unique filename that describes your recipe

**Example**: For a web scraping recipe, create `web-scraper.yaml`

### Step 3: Write Your Recipe

Use this template structure:

```yaml
# Required fields
version: 1.0.0
title: "Your Recipe Name"  # Should match your filename
description: "Brief description of what your recipe does"
instructions: "Detailed instructions for what the recipe should accomplish"
author:
  contact: "your-github-username"
extensions:
  - type: builtin
    name: developer
activities:
  - "Main activity 1"
  - "Main activity 2"
  - "Main activity 3"
prompt: |
  Detailed prompt describing the task step by step.
  
  Use {{ parameter_name }} to reference parameters.
  
  Be specific and clear about what should be done.

# Optional fields
parameters:
  - key: parameter_name
    input_type: string
    requirement: required
    description: "Description of this parameter"
    value: "default_value"
  - key: optional_param
    input_type: string
    requirement: optional
    description: "Description of optional parameter"
    default: "default_value"
```

📚 **Need help with the format?** Check out the [Recipe Reference Guide](https://goose-docs.ai/docs/guides/recipes/recipe-reference) or [existing recipes](documentation/src/pages/recipes/data/recipes/) for examples.

### Step 4: Create a Pull Request

1. **Commit your changes** in your forked repository
2. **Go to the original repository** and click "New Pull Request"
3. **Fill out the PR template**

### Step 5: Wait for Review

Our team will:
1. ✅ **Validate** your recipe automatically
2. 👀 **Review** for quality and usefulness
3. 🔒 **Security scan** (if approved for review)
4. 🎉 **Merge** your recipe!

## ✅ Recipe Requirements

Your recipe should:

- [ ] **Work correctly** - Test it before submitting
- [ ] **Be useful** - Solve a real problem or demonstrate a valuable workflow
- [ ] **Follow the format** - Refer to the [Recipe Reference Guide](https://goose-docs.ai/docs/guides/recipes/recipe-reference)
- [ ] **Have a unique filename** - No conflicts with existing recipe files

### 📝 **Naming Guidelines:**
- **Filename**: Choose a descriptive, unique filename (e.g., `web-scraper.yaml`)
- **Title**: Should match your filename (e.g., `"Web Scraper"`)

## 🔍 Recipe Validation

Your recipe will be automatically validated for:

- ✅ **Correct YAML syntax**
- ✅ **Required fields present**
- ✅ **Proper structure**
- ✅ **Security compliance**

If validation fails, you'll get helpful feedback in the PR comments.

## 🎯 Recipe Ideas

Need inspiration? Consider recipes for:

- **Web scraping** workflows
- **Data processing** pipelines
- **API integration** tasks
- **File management** automation
- **Code generation** helpers
- **Testing** and validation
- **Deployment** processes

## 🆘 Need Help?

- 📖 **Browse existing recipes** for examples
- 💬 **Ask questions** in your PR
- 🐛 **Report issues** if something isn't working
- 📚 **Check the docs** at [goose-docs.ai](https://goose-docs.ai/docs/guides/recipes/)

## 🤝 Community Guidelines

- Keep recipes focused and practical
- Share knowledge and learn from others

---

**Ready to contribute?** [Fork the repo](https://github.com/aaif-goose/goose/fork) and start creating! 

*Questions? Ask in your PR or hop into [discord](https://discord.gg/goose-oss) - we're here to help!* 💙
