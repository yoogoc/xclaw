# config

```yaml
llms:
  - name: a1
    anthropic:
      token: xxx
      # base_url: 默认https://api.anthropic.com
    model: opus-4-6
  - name: a2
    openai:
      token: xxx
      # base_url: 默认https://api.anthropic.com
    model: gpt-5
agents:
  - name: main
    llm: a1
    defaultTools: true
    tools: [a,b,c,d]
    defaultSkills: true
    skills: [a,b,c,d]
  - name: backend-engineer
    llm: a1
    defaultTools: false
    tools: [a,b,c,d]
    defaultSkills: false
    skills: [a,b,c,d]
channels:
  - name: main
    type: discord
    token: xxx
  - name: backend-engineer
    type: discord
    token: xxx
  - name: xx
    type: matrix
    token: xxx
chat_rooms:
  - type: discord
    bindings:
      - agent: main
        channel: main
        requireMention: true
      - agent: backend-engineer
        channel: backend-engineer
        requireMention: false
```
