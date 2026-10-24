//! PT: União e busca (union-find) com compressão de caminho, usada nas redes elétricas e de fluido.
//! EN: Union-find with path compression, used by the electric and fluid networks.

pub(crate) fn find(parent: &mut [usize], mut node: usize) -> usize {
    while let Some(&next) = parent.get(node) {
        if next == node {
            break;
        }
        let grand = parent.get(next).copied().unwrap_or(next);
        if let Some(slot) = parent.get_mut(node) {
            *slot = grand;
        }
        node = next;
    }
    node
}

pub(crate) fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb
        && let Some(slot) = parent.get_mut(ra)
    {
        *slot = rb;
    }
}
