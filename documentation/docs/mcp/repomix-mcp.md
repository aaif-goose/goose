---
title: Repomix Extension
description: Add Repomix MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import YouTubeShortEmbed from '@site/src/components/YouTubeShortEmbed';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

<YouTubeShortEmbed videoUrl="https://www.youtube.com/embed/69h4LLkIg_E" />


This tutorial covers how to add the [Repomix MCP Server](https://github.com/yamadashy/repomix) as a goose extension to enable automated repository packing, codebase analysis, architecture summaries, test generation, and code exploration, all while compressing the codebase to minimize token usage and stay within your LLM's context limits.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=-y&arg=repomix&arg=--mcp&id=repomix-mcp&name=Repomix&description=Pack%20repositories%20into%20AI-friendly%20formats%20for%20goose)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx -y repomix --mcp
  ```
  </TabItem>
</Tabs>
:::

## Configuration

:::info
Note that you'll need [Node.js](https://nodejs.org/) installed on your system to run this command, as it uses `npx`.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseDesktopInstaller
    extensionId="repomix-mcp"
    extensionName="Repomix"
    description="Pack repositories into AI-friendly formats for goose"
    command="npx"
    args={["-y", "repomix", "--mcp"]}
  />
</TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Repomix"
      description="Pack repositories into AI-friendly formats for goose"
      command="npx -y repomix --mcp"
    />
  </TabItem>
</Tabs>

## Example Usage

In this example, I’ll show you how to use goose with the Repomix Extension to analyze the compressed Axios repository. goose will focus on the `lib/core/Axios.js` file, explain its role in the architecture, spot missing tests, write new unit tests, and suggest improvements to the testing strategy and code maintainability. 

By compressing the entire codebase into an AI-friendly format, Repomix helps goose stay within token limits while providing detailed, file-specific insights.

### goose Prompt

> Review the file lib/core/Axios.js from the Axios repository and identify any missing unit tests needed to thoroughly cover its core functions and classes. Explain the role this file plays in the overall Axios architecture, how it integrates with other modules, and how well it is currently tested. Write new unit tests to improve coverage, and recommend improvements to the testing strategy or refactoring opportunities to make the code more maintainable.


### goose Output

:::note CLI

I'll help analyze the Axios.js core file and its testing coverage. Let's break this down into steps:

1. First, let's fetch and examine the Axios repository using repomix
2. Then analyze the specific file and its related tests
3. Finally provide recommendations and new unit tests

Let's start by getting the repository:

--- Pack Remote Repository ---

Now let's read the core Axios.js file and its related test files:

--- Read Repomix Output ---

Based on my analysis of the Axios codebase, I'll provide you with a comprehensive overview of the project structure and create an improved README. Let me break this down into sections:

**Role in Axios Architecture**

The `Axios.js` file is one of the most critical core components of the Axios library. It serves several key purposes:

1. Request Handling: It contains the main `Axios` class that processes and dispatches HTTP requests
2. Configuration Management: Handles merging of default configs with request-specific configs
3. Interceptor Management: Manages request and response interceptors
4. HTTP Method Convenience: Provides convenience methods for different HTTP verbs (get, post, etc.)

The file integrates with other core modules through:

- `InterceptorManager` for handling request/response interceptors
- `dispatchRequest` for making the actual HTTP requests
- `mergeConfig` for configuration processing
- `buildFullPath` for URL resolution
- `AxiosHeaders` for header management

**Current Test Coverage Analysis**

The current test coverage for Axios.js is fairly good but has some gaps. Looking at the test files:

1. `test/unit/core/Axios.js` - Focuses mainly on error handling cases, particularly un-writable error stacks
2. `test/specs/api.spec.js` - Tests the public API surface
3. `test/specs/interceptors.spec.js` - Tests interceptor functionality
4. `test/specs/instance.spec.js` - Tests instance creation and configuration

**Missing Test Coverage**
1. Error Handling:

```js
// Missing tests for error propagation in request chain
_request(configOrUrl, config) {
  // Error handling here needs more coverage
}
```

2. Config Processing:

```js
// Missing edge cases for config validation
request(configOrUrl, config) {
  // Config validation edge cases not fully tested
}
```

3. Method Aliases:

```js
// Missing tests for all HTTP method aliases with various parameter combinations
utils.forEach(['post', 'put', 'patch'], function forEachMethodWithData(method) {
  // Missing tests for these methods with different data types
});
```

4. Interceptor Chain:

```js
// Missing tests for complex interceptor chains and error scenarios
const chain = [dispatchRequest.bind(this), undefined];
chain.unshift.apply(chain, requestInterceptorChain);
chain.push.apply(chain, responseInterceptorChain);
```

**New Unit Tests**
Here are new unit tests to improve coverage:

```js
describe('core::Axios', function() {
  describe('request method', function() {
    it('should handle null/undefined config values', function(done) {
      const instance = new Axios({});
      
      instance.request({
        url: '/foo',
        params: null,
        data: undefined
      }).then(function(response) {
        expect(response.config.params).toBeNull();
        expect(response.config.data).toBeUndefined();
        done();
      }).catch(done);
    });

    it('should properly merge nested config objects', function(done) {
      const instance = new Axios({
        headers: {
          common: {
            'X-Common': 'common'
          },
          get: {
            'X-Method': 'get'
          }
        }
      });

      instance.request({
        method: 'get',
        headers: {
          'X-Custom': 'custom'
        }
      }).then(function(response) {
        expect(response.config.headers['X-Common']).toBe('common');
        expect(response.config.headers['X-Method']).toBe('get');
        expect(response.config.headers['X-Custom']).toBe('custom');
        done();
      }).catch(done);
    });
  });

  describe('interceptor chain', function() {
    it('should handle async interceptors correctly', async function() {
      const instance = new Axios({});
      const order = [];

      instance.interceptors.request.use(async function(config) {
        await new Promise(resolve => setTimeout(resolve, 100));
        order.push(1);
        return config;
      });

      instance.interceptors.request.use(async function(config) {
        await new Promise(resolve => setTimeout(resolve, 50));
        order.push(2);
        return config;
      });

      instance.interceptors.response.use(async function(response) {
        await new Promise(resolve => setTimeout(resolve, 100));
        order.push(3);
        return response;
      });

      await instance.request('/foo');
      expect(order).toEqual([2, 1, 3]); // Interceptors should execute in correct order
    });

    it('should handle interceptor errors properly', function(done) {
      const instance = new Axios({});
      const error = new Error('Interceptor Error');

      instance.interceptors.request.use(function() {
        throw error;
      });

      instance.request('/foo').catch(function(e) {
        expect(e).toBe(error);
        done();
      });
    });
  });

  describe('HTTP method aliases', function() {
    it('should handle various data types in post/put/patch', function(done) {
      const instance = new Axios({});
      const testCases = [
        {method: 'post', data: {foo: 'bar'}},
        {method: 'put', data: 'string data'},
        {method: 'patch', data: new URLSearchParams('foo=bar')},
        {method: 'post', data: new FormData()},
        {method: 'put', data: new Blob(['test'])},
        {method: 'patch', data: null}
      ];

      Promise.all(testCases.map(({method, data}) => {
        return instance[method]('/foo', data).then(response => {
          expect(response.config.data).toEqual(data);
          return response;
        });
      })).then(() => done()).catch(done);
    });
  });
});
```

**Recommendations for Testing Strategy**

1. Modular Testing:

- Break down tests into smaller, focused units
- Test each HTTP method alias separately
- Create dedicated test suites for config processing, interceptors, and error handling

2. Mock Integration Points:

- Use dependency injection for better isolation
- Mock external dependencies consistently
- Create reusable mock factories

3. Error Handling:

- Add more tests for error cases
- Test error propagation through interceptor chains
- Verify error object properties and inheritance

4. Config Processing:

- Test complex config merging scenarios
- Verify default values are applied correctly
- Test config validation edge cases

5. Interceptor Testing:

- Test complex chains of interceptors
- Verify async interceptor behavior
- Test interceptor error handling

**Refactoring Opportunities**

1. Error Handling:
```js
// Current
_request(configOrUrl, config) {
  try {
    return this.request(configOrUrl, config);
  } catch (err) {
    // Error handling
  }
}

// Suggested
async _request(configOrUrl, config) {
  try {
    return await this.request(configOrUrl, config);
  } catch (err) {
    if (err instanceof AxiosError) {
      throw err;
    }
    throw AxiosError.from(err);
  }
}
```
2. Config Processing:

```js
// Current
request(configOrUrl, config) {
  // Config processing
}

// Suggested
request(configOrUrl, config) {
  const normalizedConfig = this._normalizeConfig(configOrUrl, config);
  this._validateConfig(normalizedConfig);
  return this._processRequest(normalizedConfig);
}
```

3. Interceptor Chain:

```js
// Current
// Interceptor chain building mixed with request logic

// Suggested
class InterceptorChain {
  constructor(request, config) {
    this.request = request;
    this.config = config;
    this.chain = [];
  }

  addRequestInterceptors(interceptors) {
    // Add request interceptors
  }

  addResponseInterceptors(interceptors) {
    // Add response interceptors
  }

  execute() {
    // Execute chain
  }
}
```

These improvements would make the code more maintainable and easier to test, while also improving error handling and separation of concerns.

The current test coverage is good but could be improved in these specific areas to ensure more robust functionality. The suggested new tests and refactoring opportunities would help catch edge cases and make the code more maintainable.
:::
