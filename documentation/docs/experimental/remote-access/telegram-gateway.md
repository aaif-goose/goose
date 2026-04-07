---
title: Telegram Gateway
sidebar_position: 2
sidebar_label: Telegram Gateway
description: Chat with goose through Telegram from any device.
---

The Telegram Gateway lets you interact with goose through Telegram, enabling remote access from any device where Telegram is available.

:::warning Experimental Feature
The Gateway feature is experimental and in active development. Behavior and configuration may change in future releases.
:::

## How It Works

The Gateway connects your Telegram account to goose through a secure pairing process. Once paired, you can send messages to a Telegram bot that forwards them to goose, and receive formatted responses back in Telegram.

**Key details:**
- Uses a Telegram bot that you create and configure
- Secure pairing with a one-time code
- Supports formatted responses with code blocks and markdown
- Maintains a persistent session that auto-compacts for long conversations
- Works from any device with Telegram installed

## Prerequisites

Before setting up the Telegram Gateway, you need to create a Telegram bot:

1. Open Telegram and search for [@BotFather](https://t.me/BotFather)
2. Send `/newbot` and follow the prompts to create your bot
3. Copy the **bot token** that BotFather provides (looks like `123456789:ABCdefGHIjklMNOpqrsTUVwxyz`)

:::tip
Keep your bot token secure. Anyone with the token can control your bot.
:::

## Setup

### Desktop App

1. Open goose Desktop
2. Go to **Settings** > **Gateways**
3. In the Telegram section, enter your bot token
4. Click **Start** to activate the gateway
5. Click **Generate Pairing Code** to get a one-time pairing code

### Pair Your Telegram Account

1. Open Telegram and find your bot (search for the username you gave it)
2. Send the pairing code to the bot
3. The bot will confirm the pairing is complete

You can now chat with goose through Telegram!

### CLI Usage

You can also manage the gateway from the command line:

```bash
# Check gateway status
goose gateway status

# Start the Telegram gateway
goose gateway start telegram --bot-token YOUR_BOT_TOKEN

# Generate a pairing code
goose gateway pair telegram

# Stop the gateway
goose gateway stop telegram
```

## What You Can Do

Once paired, you can:
- Send messages to goose and receive responses
- Get formatted code blocks with syntax highlighting
- Continue conversations across multiple sessions
- Access all your configured goose extensions

## Managing Paired Users

From the Desktop app's Gateway settings, you can:
- View all paired Telegram users
- See which session each user is connected to
- Unpair users to revoke their access

## Troubleshooting

### Bot not responding
- Verify the bot token is correct
- Check that the gateway is running in goose Desktop
- Ensure your computer is awake and goose Desktop is open

### Pairing code not working
- Pairing codes expire after a few minutes—generate a new one
- Make sure you're sending the code to the correct bot

### Messages not formatting correctly
- The gateway converts goose's markdown to Telegram-compatible formatting
- Some complex formatting may be simplified for Telegram compatibility

## Additional Resources

- [Telegram Bot API Documentation](https://core.telegram.org/bots)
- [Gateway PR #7199](https://github.com/aaif-goose/goose/pull/7199)
