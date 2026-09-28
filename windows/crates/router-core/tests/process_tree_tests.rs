//! A árvore de processos vista de baixo.
//!
//! Isto existe por causa do erro mais caro do porte: nada no produto sabia em
//! que shell o usuário estava, e o `doctor` dizia "tudo certo" para quem
//! digitava `claude <grupo>` no Prompt de Comando e caía na conta errada.
//!
//! Provar que a cadeia "não trava" é fácil e não prova nada: uma cadeia vazia
//! também não trava. Por isso o teste sobe um `cmd.exe` que roda um `ping` — o
//! neto tem de enxergar o pai (o `cmd`) E o avô (este binário de teste), que é
//! exatamente a subida de dois degraus que o `doctor` faz para achar o shell.

use std::process::Command;
use std::time::{Duration, Instant};

use router_core::platform::job::spawn_contained;
use router_core::platform::process_tree::{parent_chain, parent_shell, processes};
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

/// O nome deste próprio executável de teste, como o Toolhelp o devolve.
fn own_exe_name() -> String {
    std::env::current_exe()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

/// A lista de processos é uma foto: o neto pode ainda não estar nela.
fn within<T>(limit: Duration, mut check: impl FnMut() -> Option<T>) -> Option<T> {
    let start = Instant::now();
    loop {
        if let Some(value) = check() {
            return Some(value);
        }
        if start.elapsed() > limit {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn the_snapshot_has_our_own_process() {
    let me = processes()
        .into_iter()
        .find(|entry| entry.pid == std::process::id())
        .expect("o próprio processo não apareceu na foto");
    assert!(
        me.exe.eq_ignore_ascii_case(&own_exe_name()),
        "o exe veio como {:?}",
        me.exe
    );
}

#[test]
fn the_chain_climbs_and_ends() {
    let chain = parent_chain(std::process::id());
    assert!(!chain.is_empty(), "todo processo tem pai");
    let mut seen: Vec<u32> = chain.iter().map(|entry| entry.pid).collect();
    let total = seen.len();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), total, "a cadeia repetiu um pid");
    // Teto de 12: a cadeia termina mesmo que a foto do Toolhelp esteja
    // inconsistente e um pid reciclado aponte para trás.
    assert!(total <= 12, "a cadeia passou do teto: {total}");
}

/// O `doctor` precisa achar o shell dois degraus acima, não um.
#[test]
fn a_grandchild_sees_the_parent_and_the_grandparent() {
    let mut command = Command::new("cmd.exe");
    command.args(["/d", "/c", "ping -n 60 127.0.0.1 >nul"]);
    let (mut child, job) = spawn_contained(&mut command, CREATE_NO_WINDOW).unwrap();
    let ping = within(Duration::from_secs(5), || {
        processes()
            .into_iter()
            .find(|entry| entry.parent == child.id() && entry.exe.eq_ignore_ascii_case("PING.EXE"))
    })
    .expect("o ping não subiu");

    let chain = parent_chain(ping.pid);
    assert_eq!(chain[0].pid, child.id(), "o pai do ping é o cmd");
    assert!(
        chain[0].exe.eq_ignore_ascii_case("cmd.exe"),
        "o pai veio como {:?}",
        chain[0].exe
    );
    assert_eq!(chain[1].pid, std::process::id(), "o avô somos nós");

    // O job leva o neto junto; `child.kill()` sozinho deixaria o ping de pé.
    drop(job);
    let _ = child.wait();
}

/// O pid 0 é o topo, e ninguém sobe além dele — nem entra na cadeia.
#[test]
fn the_top_of_the_tree_is_not_climbed() {
    assert!(parent_chain(0).is_empty());
    assert!(
        parent_chain(std::process::id())
            .iter()
            .all(|entry| entry.pid != 0),
        "o pid 0 não é ancestral de ninguém"
    );
}

/// Um pid que não existe não tem cadeia — e não é motivo para pânico.
#[test]
fn an_unknown_pid_has_no_chain() {
    assert!(parent_chain(u32::MAX).is_empty());
}

/// Só é útil se não explodir em ambiente nenhum: na CI o pai é um runner, não um
/// shell, e `None` é resposta legítima.
#[test]
fn asking_for_the_parent_shell_always_answers() {
    let _ = parent_shell();
}
