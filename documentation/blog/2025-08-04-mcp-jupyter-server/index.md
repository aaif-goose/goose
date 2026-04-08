---
title: "MCP Jupyter: AI-Powered Machine Learning and Data Science"
description: Enable AI agents to work directly with your Jupyter notebooks, leveraging persistent memory and stateful collaboration for enhanced ML and data science workflows
date: 2025-08-05
authors:
  - damien
  - dean
  - harrison
---

![MCP Jupyter Server](data-goose.png)

Machine learning and data science workflows are inherently iterative. You load data, explore patterns, build models, and refine your approach based on results. But traditional AI assistants lose context between interactions, forcing you to reload data and re-establish context repeatedly—making data-heavy development slow and expensive.

The [**MCP Jupyter Server**](https://github.com/block/mcp-jupyter) solves this by enabling AI agents like Goose to work directly with your Jupyter notebooks, maintaining persistent memory and state across interactions while letting the AI interact with your data through code execution rather than raw data transfer.

<!--truncate-->

<iframe class="aspect-ratio" src="https://www.youtube.com/embed/0i6gB_mWaRM" title="YouTube video player" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share" referrerpolicy="strict-origin-when-cross-origin" allowfullscreen></iframe>

## The Memory and Context Problem

Traditional AI coding assistants face a fundamental limitation: they lose context between interactions. This in part stems from their design to accomplish well-described individual tasks like refactoring a piece of code, compiling and running its tests, and committing the result version control. In contrast, data science workflows are often open-ended and require extensive exploration and interactivity. This creates several challenges for using AI with data-heavy ML workflows:

- **Constant data reloading**: Each interaction requires re-loading or describing your dataset
- **Expensive iteration**: Large datasets are slow and costly to process repeatedly
- **Context size**: The raw data may be too large to be fit in context
- **Not designed for interactive work**: It is harder to have the assistant do some steps and then transfer control back to the human or vice versa

To address these challenges, we created the MCP Jupyter Server.

## A Better Approach: Persistent State

The MCP Jupyter Server takes a different approach. Instead of losing context between interactions, it maintains persistent memory and lets the AI interact with your data through Jupyter code cell execution. This creates a powerful paradigm:

- **Persistent memory**: Variables and state are maintained across all interactions
- **Efficient iterations**: No need to reload data or re-establish context
- **Code-based data interaction**: AI explores data through execution, not given the raw data in context
- **True pair programming**: Seamless collaboration within the same notebook context

## Transforming ML Workflows

### Exploratory Data Analysis

Instead of describing your dataset, let the AI explore it directly:

> <span style={{color: '#2563eb', fontWeight: 'bold'}}>User:</span> "Help me understand this customer dataset"
> 
> <span style={{color: '#059669', fontWeight: 'bold'}}>AI:</span> "Let me explore your data structure first"
> 
> *<span style={{color: '#6b7280', fontStyle: 'italic'}}>[AI executes: df.info(), df.describe(), df.head()]</span>*
> 
> <span style={{color: '#059669', fontWeight: 'bold'}}>AI:</span> "I can see you have 50,000 customers with 12 features. The data includes demographics (age, income), behavioral metrics (purchase_frequency, avg_order_value), and some missing values in the 'phone_number' column. Would you like me to create visualizations for the key relationships, like age vs income or purchase patterns?"
> 
> <span style={{color: '#2563eb', fontWeight: 'bold'}}>User:</span> "Yes, show me the age and income distributions, and any correlations"
> 
> <span style={{color: '#059669', fontWeight: 'bold'}}>AI:</span> "Perfect! I'll create histograms for age and income, plus a correlation heatmap"
> 
> *<span style={{color: '#6b7280', fontStyle: 'italic'}}>[AI generates and displays visualizations]</span>*

The AI can see your actual column names, data types, and distributions—leading to much more relevant suggestions.

### Interactive Model Development

The AI becomes a true collaborator in your modeling process:

> <span style={{color: '#2563eb', fontWeight: 'bold'}}>User:</span> "This model isn't performing well"
> 
> <span style={{color: '#059669', fontWeight: 'bold'}}>AI:</span> "Let me check the current metrics and feature importance"
> 
> *<span style={{color: '#6b7280', fontStyle: 'italic'}}>[AI executes your existing model evaluation code]</span>*
> 
> <span style={{color: '#059669', fontWeight: 'bold'}}>AI:</span> "I see the accuracy is 72% and the most important features are 'price' and 'location'. The model seems to be overfitting - would you like me to try regularization or reduce the feature set?"
> 
> <span style={{color: '#2563eb', fontWeight: 'bold'}}>User:</span> "Try regularization first"
> 
> <span style={{color: '#059669', fontWeight: 'bold'}}>AI:</span> "I'll add L2 regularization and test different alpha values"
> 
> *<span style={{color: '#6b7280', fontStyle: 'italic'}}>[AI implements and tests changes iteratively]</span>*
> 
> <span style={{color: '#059669', fontWeight: 'bold'}}>AI:</span> "Great! With alpha=0.1, accuracy improved to 78% and the validation curve looks much better. Should I tune it further or try ensemble methods?"

### Real-time Debugging

When errors occur, the assistant has full context:

> <span style={{color: '#2563eb', fontWeight: 'bold'}}>User:</span> "I'm getting a KeyError in cell 5"
> 
> <span style={{color: '#059669', fontWeight: 'bold'}}>AI:</span> "Let me check what columns are available in your dataframe"
> 
> *<span style={{color: '#6b7280', fontStyle: 'italic'}}>[AI executes: df.columns, df.info()]</span>*
> 
> <span style={{color: '#059669', fontWeight: 'bold'}}>AI:</span> "I see the issue - you're trying to access 'customer_id' but the actual column name is 'CustomerID' with capital letters. Also, I notice you have 'purchase_date' as a string - should I convert that to datetime while I fix the KeyError?"
> 
> <span style={{color: '#2563eb', fontWeight: 'bold'}}>User:</span> "Yes, fix both issues"
> 
> <span style={{color: '#059669', fontWeight: 'bold'}}>AI:</span> "Done! I've updated the column reference and converted purchase_date to datetime. The cell should run properly now."

### Continuous Collaboration

Imagine a scenario where you are working on a new dataset:

> <span style={{color: '#2563eb', fontWeight: 'bold'}}>User:</span> *<span style={{color: '#6b7280', fontStyle: 'italic'}}>[Executes: df = pd.read_csv('sales_data.csv')]</span>*
> 
> <span style={{color: '#2563eb', fontWeight: 'bold'}}>User:</span> "Let me check the data types and look for any anomalies"
> 
> <span style={{color: '#059669', fontWeight: 'bold'}}>AI:</span> "I'll analyze your sales data structure"
> 
> *<span style={{color: '#6b7280', fontStyle: 'italic'}}>[AI executes: df.info(), df.describe(include='all')]</span>*
> 
> <span style={{color: '#059669', fontWeight: 'bold'}}>AI:</span> "I found a few issues: 'CustomerID' is stored as text but should be numeric, and there are 847 missing values in 'SalesAmount'. Would you like me to convert 'CustomerID' to numeric and fill the missing 'SalesAmount' values with the median?"
> 
> <span style={{color: '#2563eb', fontWeight: 'bold'}}>User:</span> "Yes, please do that and show me the updated info"
> 
> <span style={{color: '#059669', fontWeight: 'bold'}}>AI:</span> "Done! CustomerID is now numeric and SalesAmount gaps are filled. Your dataset is clean and ready for modeling."

*<span style={{color: '#6b7280', fontStyle: 'italic'}}>[Later in the session, after you've trained a model...]</span>*

> <span style={{color: '#2563eb', fontWeight: 'bold'}}>User:</span> "How is my model performing?"
> 
> <span style={{color: '#059669', fontWeight: 'bold'}}>AI:</span> "Let me generate the key metrics for your trained model"
> 
> *<span style={{color: '#6b7280', fontStyle: 'italic'}}>[AI accesses the existing model variable and generates evaluation metrics]</span>*
> 
> <span style={{color: '#059669', fontWeight: 'bold'}}>AI:</span> "Your Random Forest achieved 0.84 precision and 0.81 recall. The confusion matrix shows some false positives in the high-value sales category. Would you like me to adjust the decision threshold or try feature engineering?"

This fluid back-and-forth, where the assistant understands and builds upon the existing notebook state, allows for seamless collaboration without the need for constant context re-establishment or data reloading.

## Example notebook

Here you can see an example notebook that was handled by the MCP Jupyter Server.  The server 

**[📓 View the Complete Demo Notebook](https://github.com/block/mcp-jupyter/blob/main/demos/demo.ipynb)**

The demo walks through a typical data science workflow:
- **Install Missing Libraries**: Installing missing libraries for the notebook
- **Data Generation**: Creating synthetic data for analysis
- **Model Training**: Fitting a linear regression model with scikit-learn
- **Results Analysis**: Extracting model coefficients and performance metrics
- **Visualization**: Creating plots with seaborn

## Getting Started

The MCP Jupyter Server integrates seamlessly with existing workflows and can also be used with the notebook viewer in VS Code based IDEs.

For detailed setup and configuration, check out the [complete documentation](https://aaif-goose.github.io/mcp-jupyter/).


<head>
  <meta property="og:title" content="MCP Jupyter Server: Bringing Notebook Intelligence to AI Agents" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/06/24/mcp-jupyter-server" />
  <meta property="og:description" content="A powerful MCP server that enables AI agents to interact with Jupyter notebooks while preserving kernel state and variable context" />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/data-goose-7fc60ab0a13a9e9b6c22172d6ac166ab.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="MCP Jupyter Server: Bringing Notebook Intelligence to AI Agents" />
  <meta name="twitter:description" content="A powerful MCP server that enables AI agents to interact with Jupyter notebooks while preserving kernel state and variable context" />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/data-goose-7fc60ab0a13a9e9b6c22172d6ac166ab.png" />
</head>
