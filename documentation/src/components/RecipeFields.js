import React from "react";

const RecipeFields = () => {
  return (
    <ul>
      <li><strong>Title</strong> and <strong>description</strong></li>
      <li><strong>Instructions</strong> that tell goose what to do</li>
      <li><strong>Initial prompt</strong> to pre-fill the chat input</li>
      <li><strong>Advanced Options</strong> (expand to access):
        <ul>
          <li><strong>Activities</strong> to display as clickable buttons for users</li>
          <li><strong>Parameters</strong> to accept dynamic values</li>
          <li><strong>Model and provider</strong> to specify which provider and model the recipe should use</li>
          <li><strong>Extensions</strong> to select which tools are available</li>
          <li><strong>Response JSON schema</strong> for <a href="/docs/guides/recipes/session-recipes#structured-output-for-automation">structured output in automations</a></li>
          <li><strong>Subrecipes</strong> to attach existing subrecipes or create a new one inline</li>
        </ul>
      </li>
    </ul>
  );
};

export default RecipeFields;
