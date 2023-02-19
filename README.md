# Gather

Have you ever wanted to run multiple commands in the same terminal, and have
Docker Compose style logs come out? Then this is the tool for you.

Configure the commmands you want in a JSON (actually JSON5) file:

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

Environment substitution is not supported, you can add this support by calling
out to a shell:

```json5
{
  commands: {
    something: ["bash", "-c", "echo $PATH"],
  },
}
```
