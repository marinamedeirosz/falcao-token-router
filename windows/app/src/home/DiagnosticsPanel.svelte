<script lang="ts">
  // "Diagnosticar": o `router doctor` na tela, no fim da seção da integração.
  //
  // O diagnóstico era inalcançável justamente para quem mais precisava dele: o
  // app não o expunha, o `router.exe` não está no PATH, e a linha do README é
  // sintaxe de PowerShell — que dá erro no Prompt de Comando, o shell do
  // problema que o doctor passou a enxergar.
  //
  // Componente próprio, e não mais um bloco da seção, porque instalar e
  // diagnosticar são motivos de mudança diferentes (e o `TerminalSection` já
  // está perto da régua de 600 linhas).
  import * as api from "../lib/api";
  import Icon from "../lib/Icon.svelte";
  import { t } from "../lib/i18n";
  import Rich from "../lib/Rich.svelte";

  let running = $state(false);
  /** A saída crua do `doctor`: pt-BR FIXO mesmo com a interface em inglês — é
   *  o mesmo texto técnico que a CLI imprime, não string de catálogo. Por isso
   *  a nota ao lado, para quem lê a interface em inglês não achar que quebrou. */
  let output = $state<string | null>(null);
  /** O backend devolve `Err(String)` quando nem consegue rodar o binário. */
  let failure = $state<string | null>(null);

  async function run() {
    running = true;
    failure = null;
    try {
      output = await api.runDoctor();
    } catch (error) {
      output = null;
      failure = String(error);
    } finally {
      running = false;
    }
  }
</script>

<div class="diagnostics">
  <div class="head">
    <h4>{t("groups.diagnostics.title")}</h4>
    <span class="spacer"></span>
    {#if running}
      <span class="busy"><span class="spinner" aria-hidden="true"></span>{t("groups.diagnostics.running")}</span>
    {:else}
      <button class="secondary small" onclick={run}>{t("groups.diagnostics.run")}</button>
    {/if}
  </div>
  <!-- Sem ícone e sem flex: o `Rich` devolve texto e `<code>` soltos, e num
       flex cada trecho viraria uma coluna. -->
  <p class="pitch"><Rich text={t("groups.diagnostics.pitch")} /></p>

  {#if failure !== null}
    <p class="warn"><Icon name="warning" size={12} /><span>{t("groups.diagnostics.failed.format", failure)}</span></p>
  {:else if output !== null}
    <p class="note"><Icon name="info" size={12} /><span><Rich text={t("groups.diagnostics.technical")} /></span></p>
    <pre>{output}</pre>
  {/if}
</div>

<style>
  .diagnostics {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 24px;
  }
  h4 {
    margin: 0;
    font-size: var(--font-caption);
    font-weight: 600;
  }
  .spacer {
    flex: 1;
  }
  .pitch,
  .note,
  .warn {
    font-size: var(--font-caption);
    line-height: 1.45;
  }
  .note,
  .warn {
    display: flex;
    align-items: flex-start;
    gap: 6px;
  }
  .pitch,
  .note {
    color: var(--text-secondary);
  }
  .warn {
    color: var(--warning-text);
  }
  .warn span {
    color: var(--text);
  }
  .note :global(svg),
  .warn :global(svg) {
    margin-top: 2px;
  }
  pre {
    /* A saída vem com as linhas já quebradas pela CLI; caminho longo do Windows
       não pode empurrar a janela, então rola dentro do bloco. */
    max-height: 260px;
    margin: 0;
    padding: 8px 10px;
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: var(--font-small);
    line-height: 1.5;
    overflow: auto;
    user-select: text;
  }
  .busy {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--font-caption);
    color: var(--text-secondary);
  }
  .spinner {
    flex: none;
    width: 12px;
    height: 12px;
    border: 2px solid var(--track);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
