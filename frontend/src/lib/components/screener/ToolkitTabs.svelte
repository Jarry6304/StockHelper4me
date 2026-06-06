<script lang="ts">
  import {
    ACTIVE_TOOLKITS,
    DISABLED_TOOLKITS,
    type ActiveToolkit,
    type Toolkit
  } from '$lib/api';
  import { createEventDispatcher } from 'svelte';

  export let current: ActiveToolkit;

  const dispatch = createEventDispatcher<{
    select: { toolkit: ActiveToolkit };
    'disabled-click': { toolkit: string; reason: string };
  }>();

  function handleClick(tk: ActiveToolkit) {
    if (tk === current) return;
    dispatch('select', { toolkit: tk });
  }

  function handleDisabledClick(tk: string) {
    dispatch('disabled-click', {
      toolkit: tk,
      reason:
        '此 toolkit 為 MCP-only,需新增 HTTP 端點(spec CL3 — wave_impulse / monthly_trigger 走 MCP server,/screens 不提供)。'
    });
  }
</script>

<div class="tabs" role="tablist" aria-label="篩選 toolkit">
  {#each ACTIVE_TOOLKITS as tk}
    <button
      type="button"
      role="tab"
      aria-selected={tk === current}
      class:on={tk === current}
      on:click={() => handleClick(tk)}
    >
      {tk}
    </button>
  {/each}
  {#each DISABLED_TOOLKITS as tk}
    <button
      type="button"
      role="tab"
      aria-disabled="true"
      class="dis"
      title="MCP-only:需新增 HTTP 端點(CL3)"
      on:click={() => handleDisabledClick(tk)}
    >
      {tk}
    </button>
  {/each}
</div>

<style>
  .tabs {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
    background: var(--tag-bg);
    border: 1px solid var(--line);
    border-radius: 9px;
    padding: 4px;
  }

  button {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--ink-dim);
    padding: 5px 10px;
    border-radius: 6px;
    white-space: nowrap;
    background: transparent;
    border: none;
    cursor: pointer;
  }

  button:hover:not(.on):not(.dis) {
    color: var(--ink);
  }

  button.on {
    background: var(--wave);
    color: #04212b;
    font-weight: 600;
  }

  button.dis {
    color: #46566f;
    border: 1px dashed #3b4a63;
    position: relative;
    padding: 4px 10px;
  }

  button.dis::after {
    content: 'MCP-only';
    position: absolute;
    top: -7px;
    right: -6px;
    font-size: 8px;
    color: var(--new);
    background: var(--bg);
    padding: 0 3px;
  }

  button.dis:hover {
    color: var(--new);
  }
</style>
