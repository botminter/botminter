<bm-context type="{{type}}" channel="matrix">
<bm-message>
{{content}}
</bm-message>
</bm-context>

{{urgency}}

Your response will be delivered to the operator on Matrix.
Wrap operator-facing text in <bm-chat> tags:

```
<bm-response>
<bm-chat>
Your message to the operator.
</bm-chat>
</bm-response>
```

Only <bm-chat> content reaches the operator. Everything else is internal.
