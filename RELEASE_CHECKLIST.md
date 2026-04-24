# goose Release Manual Testing Checklist

## Version: {{VERSION}}

### Identify the high risk changes in this Release

```
./workflow_recipes/release_risk_check/run.sh {{VERSION}}
```

It will generate an analysis report in `/tmp/release_report_final.md` and perform testing is necessary for high risk pr changes. 


## Regression Testing

Make a copy of this document for each version and check off as steps are verified.

### Provider Testing

- [ ] Run `cd ui/desktop && pnpm run test:integration:providers` locally from the release branch and verify all providers/models work
- [ ] Launch goose, click reset providers, choose databricks and a model

### Starting Conversations

Test various ways to start a conversation:

- [ ] Open home and start a new conversation with "Hello"
  - [ ] Agent responds
  - [ ] Token count is updated after agent finishes
  - [ ] Go to history and see there is a new entry
- [ ] Go back to the main screen, start a new conversation from the hub and see that it opens a new conversation
- [ ] Open history and click the Hello conversation - verify it loads
- [ ] Add a new message to this conversation and see that it is added
- [ ] Change the working directory of an existing conversation
  - [ ] Ask "what is your working directory?"
  - [ ] Response should match the new directory
- [ ] Open a new window, click chat in left side for new chat
- [ ] Click "create a tamagotchi game" in popular chat topics to test developer extension

### Recipes

#### Create Recipe from Session

- [ ] Start a simple chat conversation like "hi"
- [ ] Click "create a recipe from this session" in the bottom chat bar
  - [ ] Recipe title, description and instructions should be filled in with details from the chat
  - [ ] Add a few activities and params (params unused indicator should update if added to instructions/prompts or activities)
  - [ ] Can launch create and run recipe - launches in a new window showing as a recipe agent chat with parameters filled in and interact with it
  - [ ] Recipe should be saved in recipe library

#### Use Existing Recipe

- [ ] Pick trip planner from recipe hub (go/gooserecipes)
  - [ ] See the warning whether to trust this recipe (only on fresh install)
  - [ ] See the form pop up
  - [ ] Fill in the form with "Africa" and "14 days"
  - [ ] Check results are reasonable
  - [ ] Ask how many days the trip is for - should say 14

#### Recipe Management

- [ ] Go to recipe manager and enter a new recipe to generate a joke
  - [ ] See that it works if you run it
  - [ ] Edit the recipe by bottom bar and click "View/Edit Recipe"
  - [ ] Make it generate a limerick instead
  - [ ] Check that the updated recipe works
  - [ ] Delete the recipe from the recipe manager
  - [ ] Verify recipe is actually deleted

#### Recipe from File

- [ ] Create a file `~/.config/goose/recipes/test-recipe.yaml` with the following content:

```yaml
recipe:
  title: test recipe again
  description: testing recipe again
  instructions: The value of test_param is {{test_param}}
  prompt: What is the value of test_param?
  parameters:
    - key: test_param
      input_type: string
      requirement: required
      description: Enter value for test_param
```

- [ ] See that it shows up in the list of installed recipes
- [ ] Launch the recipe, see that it asks for test_param
- [ ] Enter a number, see that it pre-fills the prompt and tells you the value after you hit submit
- [ ] Go to hub and enter "what is the value of test_param"
- [ ] See a new chat that says it has no idea (recipe is no longer active)

### Extensions

#### Manual Extension Addition

- [ ] Can manually add an extension using random quotes from project
  - [ ] Add new custom stdio extension with the following command and save:
    - [ ] `node /ABSOLUTE/PATH/TO/goose/ui/desktop/tests/e2e/basic-mcp.ts` (use your actual project path)
    - [ ] Should add and can chat to ask for a random quote

#### Playwright Extension

- [ ] Install the playwright extension from the extensions hub
  - [ ] Tell it to open a browser and search on Google for cats
  - [ ] Verify that the browser opens and navigates

#### Extension with Environment Variables

- [ ] Install an extension from deeplink that needs env variables:
  - [ ] Use: `goose://extension?cmd=npx&arg=-y&arg=%40upstash%2Fcontext7-mcp&id=context7&name=Context7&description=Use%20up-to-date%20code%20and%20docs&env=TEST_ACCESS_TOKEN`
  - [ ] Extension page should load with env variables modal showing
  - [ ] Allow form input and saving extension

### Speech-to-Text (Local Model)

- [ ] Go to Settings > Chat > Voice dictation provider and select the small model
- [ ] Run a quick test that speech-to-text is working (click the mic button, speak, verify transcription)
- [ ] Also try OpenAI using your OpenAI key

### Settings

- [ ] Settings page loads and all tabs load
- [ ] Can change dark mode setting

### Follow-up Issues

Link any GitHub issues filed during testing:

---

**Tested by:** _____
**Date:** _____
**Notes:** _____
