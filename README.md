# Shunt

Have you ever wanted to run multiple commands in the same terminal, and have
Docker Compose style logs come out? Then this is the tool for you.

Configure the commands you want in a JSON (actually JSON5) _shunt file_:

```json5
// dev.json
{
  commands: {
    ui: ["npm", "run", "dev"],
    backend: {
      argv: ["cargo", "run"],
      workdir: "server"
  },
}
```

Run `shunt dev.json` and see your output neatly woven together similar to
Docker Compose. Commands inherit the current terminals exported environment like
normal child processes would.

## Workdir

You can set the working directory that the command will run in, relative paths
are relative **to the JSON config**.

```json
{
  "commands": {
    "ui": {
      "argv": ["npm", "run", "dev"],
      "workdir": "frontend"
    }
  }
}
```

## Pseudo-TTY

Each command is run in a pseudo-TTY if `shunt` itself is run in a pseudo-TTY.
You can disable this with the `tty` option on a command:

```json
{
  "commands": {
    "example-command": {
      "argv": "./build.sh",
      "tty": "never"
    }
  }
}
```

`tty` can be `auto`, `never`, or `always`.

## Environment substitution

Environment substitution is not supported, you can add this support by calling
out to a shell:

```json5
{
  commands: {
    something: ["bash", "-c", "echo $PATH"],
  },
}
```

## Modifying environment

Commands inherit the environment of the current terminal, but you can **update**
the environment with the `env` option.

```json5
{
  commands: {
    ui: {
      argv: ["npm", "start"],
      env: {
        BROWSER: "none",
        API_CRED: null, // unset a variable.
      },
    },
  },
}
```

# TODO

- [ ] Properly lock stdout in order to remove any chance of tearing.
- [ ] Add global option for colored prefixes (this wouldn't stop color from
      children).
- [ ] argv splitting option, eg just provide "echo hello".
- [ ] (maybe) be able to send input.
- [ ] Env substitution, esp in 'env' field.
