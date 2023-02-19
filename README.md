# Shunt

Have you ever wanted to run multiple commands in the same terminal, and have
Docker Compose style logs come out? Then this is the tool for you.

Configure the commands you want in a JSON (actually JSON5) file:

```json5
// dev.json
{
  commands: {
    ui: ["npm", "run", "dev"],
    backend: ["cargo", "run"],
  },
}
```

Run `gather dev.json` and see your output neatly woven together similar to
Docker Compose.

## Workdir

You can set the working directory that the command will run in, relative paths
are relative _to the JSON config_.

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

Each command is run in a pseudo-TTY if `gather` itself is run in a pseudo-TTY.
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

# TODO

- [ ] Set cwd for command.
- [ ] Add global option for colored prefixes (this wouldn't stop color from
      children).
- [ ] argv splitting option, eg just provide "echo hello".
- [ ] (maybe) be able to send input.
