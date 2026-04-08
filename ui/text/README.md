# goose ACP TUI

Early stage and part of goose's broader move to ACP

https://github.com/aaif-goose/goose/issues/6642
https://github.com/aaif-goose/goose/discussions/7309

## Running

The TUI automatically launches the goose ACP server using the `goose acp` command.

```bash
cd ui/text
npm i
npm run start
```

To use a custom server URL instead:

```bash
npm run start -- --server http://localhost:8080
```
