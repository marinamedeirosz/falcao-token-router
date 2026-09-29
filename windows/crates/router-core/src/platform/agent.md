# platform — o específico do Windows

Funções pequenas atrás das quais mora o que é da plataforma.

## Arquivos
- `atomic_write.rs` — `write_atomic`: grava num temporário na mesma pasta e renomeia por
  cima. O `rename` é atômico no Win 10+ e, por ser de arquivo recém-escrito, deixa **mtime
  novo** — o que a credencial exige (o Claude Code relê a credencial quando o mtime muda).
  Nunca `CopyFile`, que preservaria o mtime da origem. `retrying`/`read_retrying`/
  `remove_retrying`: nova tentativa (~0,6 s) nos erros 5/32/33/1224 — outro processo com o
  arquivo aberto sem compartilhar (antivírus, indexador, o próprio Claude Code).
- `json_file.rs` — `edit_object`: edição cirúrgica de JSON-objeto (`.claude.json`,
  `settings.json`): preserva ordem e chaves, recuo 2, **recusa** arquivo ilegível (nunca troca
  por `{}`), ausente/vazio = objeto novo.
- `named_mutex.rs` — `acquire(nome, prazo)`: mutex nomeado (`CreateMutexW`), trava entre
  processos. Guarda `!Send` (a posse é da thread); mutex abandonado por dono morto é assumido.
- `process_times.rs` — `probe(pid)` (`OpenProcess` + `GetExitCodeProcess` + `GetProcessTimes`:
  `Missing`/`Running(início)`/`Denied` = existe mas é de outro dono) e FILETIME ↔ data.
- `host.rs` — `local_host_names`: nome DNS (`GetComputerNameExW`) e `%COMPUTERNAME%`, para o
  `pidDomain` (`win32:<host>` em minúsculas, visto no spike).
- `paths.rs` — comparação de caminho do Windows sem tocar o disco (`normalized`,
  `is_strictly_inside`: sem caixa, `/` = `\`, `..` desqualifica).
- `short_path.rs` — nome 8.3 (`GetShortPathNameW`) de um caminho que existe; `None` se o 8.3
  está desligado no volume (ainda sobra espaço no nome).
- `ui_language.rs` — o idioma da interface (`GetUserDefaultUILanguage`): qualquer português
  conta como português. Um critério só para o app (catálogo) e a CLI (dias da semana na
  status line) — 23/09/2026, saiu do app para o núcleo.
- `git_bash.rs` — `find_git_bash`: o bash que o Claude Code usa para a status line, na ordem
  dele (`CLAUDE_CODE_GIT_BASH_PATH` — só se for um `bash`/`sh` — → Program Files → Program
  Files (x86) → git do PATH).
- `powershell.rs` — `find_powershell`: o PowerShell que ele usa sem Git Bash, na ordem dele
  (`pwsh` do PATH → pastas do PowerShell 7, inclusive o atalho da Store → `powershell` do PATH
  → o 5.1 do System32).
- `job.rs` — `spawn_contained`: sobe um processo SUSPENSO, põe num Job Object com
  `KILL_ON_JOB_CLOSE` e só então o solta (a thread vem da lista Toolhelp) — nenhum neto nasce
  fora. `Job::terminate` mata a árvore; fechar o job (ou morrer quem o criou) também. Sem job
  (o sistema recusou), sobe mesmo assim. `test_support` vê a árvore nos testes.
- `known_folders.rs` — `documents_dir`: a Documentos real (`SHGetKnownFolderPath`), que com o
  OneDrive não é `%USERPROFILE%\Documents` — é onde moram os `$PROFILE`.
- `profile_append.rs` — `append_block`: acrescenta ao perfil do shell **em bytes**, na
  codificação do BOM (UTF-16LE/BE, UTF-8; sem BOM o bloco é ASCII, igual em ANSI) e no fim de
  linha do arquivo; modo append (segue link, não troca o arquivo); arquivo ilegível = erro.
  `remove_block`: tira o bloco. Grava **no lugar** (truncate+write), nunca `write_atomic` — o
  rename trocaria um `$PROFILE` que é link simbólico pelo arquivo novo, e o perfil de verdade
  ficaria órfão. Corta por **faixa de bytes**, não decodifica-edita-regrava: `decode` é lossy,
  e um perfil ANSI com acento voltaria ao disco com U+FFFD. `decode_profile`: a ÚNICA leitura
  de BOM completa do repositório — havia três cópias e só esta trata UTF-16**BE**, o que
  deixava o botão "Ativar" nunca ficar verde num perfil salvo assim.
- `command_processor.rs` — o `AutoRun` do Prompt de Comando
  (`HKCU\Software\Microsoft\Command Processor`), espelho do `profile_append` para o registro:
  o cmd não tem `$PROFILE`, e é dali que a macro `doskey claude` nasce. **Compõe, nunca
  sobrescreve**: clink, ConEmu e Anaconda põem o deles no mesmo valor, e o separador de
  terceiro volta como veio — trocar `&&` por `&` faria rodar sempre o que só devia rodar no
  sucesso. Lê `REG_SZ` e `REG_EXPAND_SZ` (com `RRF_NOEXPAND`) e regrava o mesmo tipo: ler só
  um faria o valor do outro parecer ausente e ser atropelado. Chave injetável — o teste não
  toca o AutoRun da máquina.
- `process_tree.rs` — `processes`/`parent_chain`/`parent_shell`: em que shell o processo
  nasceu. Existe porque NADA no produto sabia isso, e o `doctor` dizia "tudo certo" no único
  caso que não enxergava (o usuário no cmd). `parent_chain` para por teto **e** por conjunto
  de visitados: a foto do Toolhelp não é atômica e pid reciclado fecha ciclo.
- `console.rs` — o Ctrl+C no `router launch`. Sem `exec` no Windows, o `router` espera o
  filho; um Ctrl+C vai para TODOS os processos do console, e se o `router` morresse o shell
  voltaria ao prompt com o `claude` rodando por baixo. Handler PRÓPRIO devolvendo TRUE — o
  atalho `SetConsoleCtrlHandler(NULL, TRUE)` liga um atributo HERDADO, e o `claude` passaria
  a ignorar Ctrl+C também.
- `process.rs` — `run_with_timeout`: comando curto com prazo (stdout por thread, mata ao
  estourar), sem console (`CREATE_NO_WINDOW` — do app cada consulta piscaria uma janela).
- `links.rs` — `junction` (pastas, sem privilégio), `symlink_file` (flag
  `ALLOW_UNPRIVILEGED_CREATE`; sem Developer Mode falha com 1314), `is_junction`/`is_symlink`,
  `developer_mode_enabled` (registro `AppModelUnlock`, só para dica na UI/`doctor`).

## Decisões
- 22/09/2026: hardlink NÃO para `history.jsonl` — o binário 2.1.280 reescreve o arquivo comum
  na poda de retenção (e pula link), e o hardlink divergiria em silêncio.
- 23/09/2026: o comando de status line do usuário roda num Job Object — o `Child::kill` só
  mata o shell, e um `node` pendurado sobraria a cada render (o Windows não tem grupo de
  processos). Criado suspenso para nenhum neto escapar entre o `spawn` e a entrada no job.

- 28/09/2026: a enumeração Toolhelp saiu do `#[cfg(test)] mod test_support` do `job.rs` para
  o `process_tree.rs` de produção, e o `test_support` passou a chamá-la. Era a única
  implementação e estava do lado errado da cerca.

## Pendências
- `write_atomic` **troca** um symlink/junction do usuário por arquivo comum (o rename não
  segue link). O `profile_append::remove_block` já contorna gravando no lugar; os demais
  chamadores ainda não.
- Nada usa o prefixo `\\?\`: caminho acima de 260 não é tratado, e os perfis de grupo são
  fundos.
- `run_with_timeout` mata só o filho direto, tendo o `job::spawn_contained` na pasta ao lado;
  e decodifica a saída como UTF-8, que não é o que o console do Windows entrega.
