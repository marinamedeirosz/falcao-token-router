//! `router uninstall-integration` — desfaz tudo o que a integração escreveu.
//!
//! Até aqui o produto só sabia INSTALAR. A remoção era um passo manual no
//! README ("abra o `notepad $PROFILE` e apague a linha"), com três consequências
//! ruins: quem desinstalava o app ficava com uma função `claude` quebrada
//! avisando em vermelho para sempre; não havia como o app oferecer "Desativar";
//! e nenhuma correção podia MOVER a linha de lugar, só acrescentar.
//!
//! Três donos chamam isto: o botão "Desativar" da tela Grupos, o
//! `NSIS_HOOK_PREUNINSTALL` (que roda com o `router.exe` ainda no lugar) e o
//! usuário pela linha de comando.

/// TRILHA T10: remover o segmento do `AutoRun` (`command_processor::remove`), a
/// linha de cada `$PROFILE` e a do `.bashrc` (`profile_append::remove_block`).
/// Devolve `true` quando tudo saiu — best-effort, e o que falhou é nomeado.
pub fn run() -> bool {
    eprintln!("router uninstall-integration: ainda não implementado");
    false
}
