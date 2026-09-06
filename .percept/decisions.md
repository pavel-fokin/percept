# decisions

Folded from the percept log for this project and rerendered on every write. Change it with `percept maps`, not by hand.

## Overview

Shared interpretations; agreement is unknown unless cited explicitly. Open an entry for its rationale, supporting choices, and relationships.

<ul>
<li><a href="#node-01a07559-4d29-7ae3-8e77-950333eb01fa">commitment: Event storage</a></li>
<li><a href="#node-01a07559-4d5e-7023-92bc-9a7faa735b63">commitment: Command suggestions</a></li>
<li><a href="#node-01a07559-4d8d-7a11-b2cd-17dc98d44905">commitment: Fireworks integration</a></li>
<li><a href="#node-01a07559-4db8-7092-a45c-b57e67ea081b">commitment: Stable shared maps</a></li>
</ul>

<details id="node-01a07559-4d29-7ae3-8e77-950333eb01fa">
<summary>commitment: Event storage</summary>

<div>
<p><strong>commitment: Event storage</strong></p>
<dl>
<dt>scope</dt><dd>Grouping of existing project decisions; original choices remain below.</dd>
<dt>status</dt><dd>proposed</dd>
<dt>why</dt><dd>Keep installed use in one shared log, with project scoping and a development-build exception.</dd>
</dl>
<p>Node sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b, 01a0705c-72fd-70b1-a4c6-0408b12bd8d5, 01a07195-c647-7f70-87ee-fb8f9faacdd8, 01a07195-da7b-72e3-9160-1577b943f1c4</code></p>
</div>
<div id="node-01a0705c-7302-7d52-8fd9-6fa4fb81f970">
<p><strong>question: Where does the event log live?</strong></p>
<p>Node sources: <code>01a0705c-72fd-70b1-a4c6-0408b12bd8d5</code></p>
</div>
<div id="node-01a0705c-7307-7ba2-a8fa-bc5b24d32fe3">
<p><strong>option: percept.jsonl in the working directory</strong></p>
<p>Node sources: <code>01a0705c-72fd-70b1-a4c6-0408b12bd8d5</code></p>
</div>
<div id="node-01a0705c-730c-7a40-b52a-618500191d16">
<p><strong>option: one log under ~/.percept</strong></p>
<p>Node sources: <code>01a0705c-72fd-70b1-a4c6-0408b12bd8d5</code></p>
</div>
<div id="node-01a0705c-7310-7122-942d-327dbf1935bf">
<p><strong>decision: one log under ~/.percept</strong></p>
<dl>
<dt>why</dt><dd>PERCEPT_HOME also holds the binary; one log keeps cross-project search free; an event&#39;s source.path says which project</dd>
</dl>
<p>Node sources: <code>01a0705c-72fd-70b1-a4c6-0408b12bd8d5</code></p>
</div>
<div id="node-01a07195-f4bd-7da0-9f64-46fe124e1ff9">
<p><strong>question: Where should a dev build&#39;s event log default to when PERCEPT_HOME is unset?</strong></p>
<p>Node sources: <code>01a07195-c647-7f70-87ee-fb8f9faacdd8</code></p>
</div>
<div id="node-01a07195-f4cf-79b0-8016-cfd88be4a59c">
<p><strong>option: always ~/.percept, PERCEPT_HOME is the only override</strong></p>
<p>Node sources: <code>01a07195-c647-7f70-87ee-fb8f9faacdd8</code></p>
</div>
<div id="node-01a07195-f4e0-7bd2-9146-c9d87ca0b2c8">
<p><strong>option: detect target/debug or target/release via current_exe(), default those to &lt;checkout&gt;/.percept</strong></p>
<p>Node sources: <code>01a07195-da7b-72e3-9160-1577b943f1c4</code></p>
</div>
<div id="node-01a07195-f4ef-76e1-ab04-dffb94b8b33e">
<p><strong>decision: detect target/debug or target/release via current_exe(), default those to &lt;checkout&gt;/.percept</strong></p>
<dl>
<dt>why</dt><dd>install.sh copies the binary to ~/.percept/bin, outside target/, so the check separates a repo build from an installed one without a build-time flag; PERCEPT_HOME still overrides either case</dd>
</dl>
<p>Node sources: <code>01a07195-da7b-72e3-9160-1577b943f1c4</code></p>
</div>
<p><a href="#node-01a0705c-7310-7122-942d-327dbf1935bf">decision: one log under ~/.percept</a> <strong>resolves</strong> <a href="#node-01a0705c-7302-7d52-8fd9-6fa4fb81f970">question: Where does the event log live?</a></p>
<p>Relationship sources: <code>01a0705c-72fd-70b1-a4c6-0408b12bd8d5</code></p>
<p><a href="#node-01a07195-f4ef-76e1-ab04-dffb94b8b33e">decision: detect target/debug or target/release via current_exe(), default those to &lt;checkout&gt;/.percept</a> <strong>resolves</strong> <a href="#node-01a07195-f4bd-7da0-9f64-46fe124e1ff9">question: Where should a dev build&#39;s event log default to when PERCEPT_HOME is unset?</a></p>
<p>Relationship sources: <code>01a07195-da7b-72e3-9160-1577b943f1c4</code></p>
<p><a href="#node-01a07559-4d29-7ae3-8e77-950333eb01fa">commitment: Event storage</a> <strong>details</strong> <a href="#node-01a0705c-7302-7d52-8fd9-6fa4fb81f970">question: Where does the event log live?</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d29-7ae3-8e77-950333eb01fa">commitment: Event storage</a> <strong>details</strong> <a href="#node-01a0705c-7307-7ba2-a8fa-bc5b24d32fe3">option: percept.jsonl in the working directory</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d29-7ae3-8e77-950333eb01fa">commitment: Event storage</a> <strong>details</strong> <a href="#node-01a0705c-730c-7a40-b52a-618500191d16">option: one log under ~/.percept</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d29-7ae3-8e77-950333eb01fa">commitment: Event storage</a> <strong>details</strong> <a href="#node-01a0705c-7310-7122-942d-327dbf1935bf">decision: one log under ~/.percept</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d29-7ae3-8e77-950333eb01fa">commitment: Event storage</a> <strong>details</strong> <a href="#node-01a07195-f4bd-7da0-9f64-46fe124e1ff9">question: Where should a dev build&#39;s event log default to when PERCEPT_HOME is unset?</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d29-7ae3-8e77-950333eb01fa">commitment: Event storage</a> <strong>details</strong> <a href="#node-01a07195-f4cf-79b0-8016-cfd88be4a59c">option: always ~/.percept, PERCEPT_HOME is the only override</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d29-7ae3-8e77-950333eb01fa">commitment: Event storage</a> <strong>details</strong> <a href="#node-01a07195-f4e0-7bd2-9146-c9d87ca0b2c8">option: detect target/debug or target/release via current_exe(), default those to &lt;checkout&gt;/.percept</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d29-7ae3-8e77-950333eb01fa">commitment: Event storage</a> <strong>details</strong> <a href="#node-01a07195-f4ef-76e1-ab04-dffb94b8b33e">decision: detect target/debug or target/release via current_exe(), default those to &lt;checkout&gt;/.percept</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>

</details>

<details id="node-01a07559-4d5e-7023-92bc-9a7faa735b63">
<summary>commitment: Command suggestions</summary>

<div>
<p><strong>commitment: Command suggestions</strong></p>
<dl>
<dt>scope</dt><dd>Grouping of existing project decisions; original choices remain below.</dd>
<dt>status</dt><dd>proposed</dd>
<dt>why</dt><dd>Keep suggestions close to the input and preserve Enter behavior while the list is open.</dd>
</dl>
<p>Node sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b, 01a070b1-9154-7880-82a6-2ad9b58e89f0, 01a070cc-c462-76d2-b536-3eae30e28f0f</code></p>
</div>
<div id="node-01a070cd-3304-7633-9574-4071d7305010">
<p><strong>question: Where should the command suggestion list render?</strong></p>
<p>Node sources: <code>01a070cc-c462-76d2-b536-3eae30e28f0f</code></p>
</div>
<div id="node-01a070cd-3313-7310-9705-20cf1a473fea">
<p><strong>option: centered popup like the existing ModelsMenu</strong></p>
<p>Node sources: <code>01a070cc-c462-76d2-b536-3eae30e28f0f</code></p>
</div>
<div id="node-01a070cd-3320-7291-a242-4fa37986f55d">
<p><strong>option: anchored above the input box</strong></p>
<p>Node sources: <code>01a070cc-c462-76d2-b536-3eae30e28f0f</code></p>
</div>
<div id="node-01a070cd-332c-7f32-8810-0b0c9db787db">
<p><strong>decision: anchored above the input box</strong></p>
<dl>
<dt>why</dt><dd>input sits between the transcript and the status bar, so suggestions render in that gap, matching Claude Code/Codex</dd>
</dl>
<p>Node sources: <code>01a070cc-c462-76d2-b536-3eae30e28f0f</code></p>
</div>
<div id="node-01a070cd-3344-7360-9f37-0b229c9d429a">
<p><strong>question: Which key accepts a highlighted suggestion?</strong></p>
<p>Node sources: <code>01a070cc-c462-76d2-b536-3eae30e28f0f</code></p>
</div>
<div id="node-01a070cd-334f-7e42-84d5-dd19496da878">
<p><strong>option: Tab only</strong></p>
<p>Node sources: <code>01a070cc-c462-76d2-b536-3eae30e28f0f</code></p>
</div>
<div id="node-01a070cd-335a-7681-99fe-a44a3513217b">
<p><strong>option: Enter only</strong></p>
<p>Node sources: <code>01a070cc-c462-76d2-b536-3eae30e28f0f</code></p>
</div>
<div id="node-01a070cd-3364-72d0-91d4-07c2acd65357">
<p><strong>option: both Tab and Enter</strong></p>
<p>Node sources: <code>01a070cc-c462-76d2-b536-3eae30e28f0f</code></p>
</div>
<div id="node-01a070cd-336f-7b00-8dcb-fc99d2f17ab9">
<p><strong>decision: Tab only</strong></p>
<dl>
<dt>why</dt><dd>Enter keeps its existing submit/execute behavior, so opening the dropdown doesn&#39;t change what Enter does</dd>
</dl>
<p>Node sources: <code>01a070cc-c462-76d2-b536-3eae30e28f0f</code></p>
</div>
<p><a href="#node-01a070cd-332c-7f32-8810-0b0c9db787db">decision: anchored above the input box</a> <strong>resolves</strong> <a href="#node-01a070cd-3304-7633-9574-4071d7305010">question: Where should the command suggestion list render?</a></p>
<p>Relationship sources: <code>01a070cc-c462-76d2-b536-3eae30e28f0f</code></p>
<p><a href="#node-01a070cd-336f-7b00-8dcb-fc99d2f17ab9">decision: Tab only</a> <strong>resolves</strong> <a href="#node-01a070cd-3344-7360-9f37-0b229c9d429a">question: Which key accepts a highlighted suggestion?</a></p>
<p>Relationship sources: <code>01a070cc-c462-76d2-b536-3eae30e28f0f</code></p>
<p><a href="#node-01a07559-4d5e-7023-92bc-9a7faa735b63">commitment: Command suggestions</a> <strong>details</strong> <a href="#node-01a070cd-3304-7633-9574-4071d7305010">question: Where should the command suggestion list render?</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d5e-7023-92bc-9a7faa735b63">commitment: Command suggestions</a> <strong>details</strong> <a href="#node-01a070cd-3313-7310-9705-20cf1a473fea">option: centered popup like the existing ModelsMenu</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d5e-7023-92bc-9a7faa735b63">commitment: Command suggestions</a> <strong>details</strong> <a href="#node-01a070cd-3320-7291-a242-4fa37986f55d">option: anchored above the input box</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d5e-7023-92bc-9a7faa735b63">commitment: Command suggestions</a> <strong>details</strong> <a href="#node-01a070cd-332c-7f32-8810-0b0c9db787db">decision: anchored above the input box</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d5e-7023-92bc-9a7faa735b63">commitment: Command suggestions</a> <strong>details</strong> <a href="#node-01a070cd-3344-7360-9f37-0b229c9d429a">question: Which key accepts a highlighted suggestion?</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d5e-7023-92bc-9a7faa735b63">commitment: Command suggestions</a> <strong>details</strong> <a href="#node-01a070cd-334f-7e42-84d5-dd19496da878">option: Tab only</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d5e-7023-92bc-9a7faa735b63">commitment: Command suggestions</a> <strong>details</strong> <a href="#node-01a070cd-335a-7681-99fe-a44a3513217b">option: Enter only</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d5e-7023-92bc-9a7faa735b63">commitment: Command suggestions</a> <strong>details</strong> <a href="#node-01a070cd-3364-72d0-91d4-07c2acd65357">option: both Tab and Enter</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d5e-7023-92bc-9a7faa735b63">commitment: Command suggestions</a> <strong>details</strong> <a href="#node-01a070cd-336f-7b00-8dcb-fc99d2f17ab9">decision: Tab only</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>

</details>

<details id="node-01a07559-4d8d-7a11-b2cd-17dc98d44905">
<summary>commitment: Fireworks integration</summary>

<div>
<p><strong>commitment: Fireworks integration</strong></p>
<dl>
<dt>scope</dt><dd>Grouping of existing project decisions; original choices remain below.</dd>
<dt>status</dt><dd>proposed</dd>
<dt>why</dt><dd>Use a dedicated provider adapter so protocol parsing and credentials have a clear owner.</dd>
</dl>
<p>Node sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b, 01a072d8-2b12-79e3-abc2-5ab899f9b15c, 01a072dd-7ffc-7112-94d3-c7137849d2f2</code></p>
</div>
<div id="node-01a072dd-b777-79f0-9e25-aa6acc534cfd">
<p><strong>question: Which Fireworks model should PERCEPT_PROVIDER=fireworks build?</strong></p>
<p>Node sources: <code>01a072dd-7ffc-7112-94d3-c7137849d2f2</code></p>
</div>
<div id="node-01a072dd-b78b-75c1-a624-b64799c15f43">
<p><strong>decision: accounts/fireworks/models/glm-5p3</strong></p>
<dl>
<dt>why</dt><dd>one static model like OPENAI_MODEL; Fireworks confirms Function Calling supported, no separate reasoning output</dd>
</dl>
<p>Node sources: <code>01a072dd-7ffc-7112-94d3-c7137849d2f2</code></p>
</div>
<div id="node-01a072dd-b7ab-7c52-ba31-a32e6c23b3a7">
<p><strong>question: How should the Fireworks provider be built?</strong></p>
<p>Node sources: <code>01a072dd-7ffc-7112-94d3-c7137849d2f2</code></p>
</div>
<div id="node-01a072dd-b7ba-7210-a87a-4ff4b828ca8d">
<p><strong>option: new Fireworks struct with its own wire parser</strong></p>
<p>Node sources: <code>01a072dd-7ffc-7112-94d3-c7137849d2f2</code></p>
</div>
<div id="node-01a072dd-b7c7-7572-bae4-dca4cc990eae">
<p><strong>option: reuse OpenAi struct with a different base URL</strong></p>
<p>Node sources: <code>01a072dd-7ffc-7112-94d3-c7137849d2f2</code></p>
</div>
<div id="node-01a072dd-b7d5-7c12-9e05-274ab8734bd3">
<p><strong>decision: new Fireworks struct with its own wire parser</strong></p>
<dl>
<dt>why</dt><dd>Fireworks speaks classic /chat/completions SSE, not OpenAi&#39;s /responses shape</dd>
</dl>
<p>Node sources: <code>01a072dd-7ffc-7112-94d3-c7137849d2f2</code></p>
</div>
<div id="node-01a072dd-b7f0-7270-b6fa-f724e43f386d">
<p><strong>question: Where do the Fireworks base URL and API key come from?</strong></p>
<p>Node sources: <code>01a072dd-7ffc-7112-94d3-c7137849d2f2</code></p>
</div>
<div id="node-01a072dd-b7fe-72a0-946b-2ac88ae95bf7">
<p><strong>decision: hardcoded https://api.fireworks.ai/inference/v1, FIREWORKS_API_KEY env var</strong></p>
<dl>
<dt>why</dt><dd>follows OPENAI_URL/OPENAI_KEY_VAR pattern: not env-configurable URL, eager key check in build_model, lenient in build_catalog</dd>
</dl>
<p>Node sources: <code>01a072dd-7ffc-7112-94d3-c7137849d2f2</code></p>
</div>
<p><a href="#node-01a072dd-b78b-75c1-a624-b64799c15f43">decision: accounts/fireworks/models/glm-5p3</a> <strong>resolves</strong> <a href="#node-01a072dd-b777-79f0-9e25-aa6acc534cfd">question: Which Fireworks model should PERCEPT_PROVIDER=fireworks build?</a></p>
<p>Relationship sources: <code>01a072dd-7ffc-7112-94d3-c7137849d2f2</code></p>
<p><a href="#node-01a072dd-b7d5-7c12-9e05-274ab8734bd3">decision: new Fireworks struct with its own wire parser</a> <strong>resolves</strong> <a href="#node-01a072dd-b7ab-7c52-ba31-a32e6c23b3a7">question: How should the Fireworks provider be built?</a></p>
<p>Relationship sources: <code>01a072dd-7ffc-7112-94d3-c7137849d2f2</code></p>
<p><a href="#node-01a072dd-b7fe-72a0-946b-2ac88ae95bf7">decision: hardcoded https://api.fireworks.ai/inference/v1, FIREWORKS_API_KEY env var</a> <strong>resolves</strong> <a href="#node-01a072dd-b7f0-7270-b6fa-f724e43f386d">question: Where do the Fireworks base URL and API key come from?</a></p>
<p>Relationship sources: <code>01a072dd-7ffc-7112-94d3-c7137849d2f2</code></p>
<p><a href="#node-01a07559-4d8d-7a11-b2cd-17dc98d44905">commitment: Fireworks integration</a> <strong>details</strong> <a href="#node-01a072dd-b777-79f0-9e25-aa6acc534cfd">question: Which Fireworks model should PERCEPT_PROVIDER=fireworks build?</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d8d-7a11-b2cd-17dc98d44905">commitment: Fireworks integration</a> <strong>details</strong> <a href="#node-01a072dd-b78b-75c1-a624-b64799c15f43">decision: accounts/fireworks/models/glm-5p3</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d8d-7a11-b2cd-17dc98d44905">commitment: Fireworks integration</a> <strong>details</strong> <a href="#node-01a072dd-b7ab-7c52-ba31-a32e6c23b3a7">question: How should the Fireworks provider be built?</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d8d-7a11-b2cd-17dc98d44905">commitment: Fireworks integration</a> <strong>details</strong> <a href="#node-01a072dd-b7ba-7210-a87a-4ff4b828ca8d">option: new Fireworks struct with its own wire parser</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d8d-7a11-b2cd-17dc98d44905">commitment: Fireworks integration</a> <strong>details</strong> <a href="#node-01a072dd-b7c7-7572-bae4-dca4cc990eae">option: reuse OpenAi struct with a different base URL</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d8d-7a11-b2cd-17dc98d44905">commitment: Fireworks integration</a> <strong>details</strong> <a href="#node-01a072dd-b7d5-7c12-9e05-274ab8734bd3">decision: new Fireworks struct with its own wire parser</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d8d-7a11-b2cd-17dc98d44905">commitment: Fireworks integration</a> <strong>details</strong> <a href="#node-01a072dd-b7f0-7270-b6fa-f724e43f386d">question: Where do the Fireworks base URL and API key come from?</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>
<p><a href="#node-01a07559-4d8d-7a11-b2cd-17dc98d44905">commitment: Fireworks integration</a> <strong>details</strong> <a href="#node-01a072dd-b7fe-72a0-946b-2ac88ae95bf7">decision: hardcoded https://api.fireworks.ai/inference/v1, FIREWORKS_API_KEY env var</a></p>
<p>Relationship sources: <code>01a0754e-664e-7650-ad6f-2aff9178471b</code></p>

</details>

<details id="node-01a07559-4db8-7092-a45c-b57e67ea081b">
<summary>commitment: Stable shared maps</summary>

<div>
<p><strong>commitment: Stable shared maps</strong></p>
<dl>
<dt>scope</dt><dd>Both human and agent interpretations; shared guidance and capture across Claude Code and Codex.</dd>
<dt>status</dt><dd>agreed</dd>
<dt>why</dt><dd>Record lasting rationale, preserve familiar references, and make evidence and disagreement inspectable.</dd>
</dl>
<p>Node sources: <code>01a0754e-6649-7f43-aa25-2ed3d730bebe, 01a0754e-664c-7fa0-a2c6-514182e11940</code></p>
</div>

</details>
