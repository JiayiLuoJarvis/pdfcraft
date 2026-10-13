//! Protocol checks for one chart: ids, edges, cycles, the two cones, and parent ratios.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::{Error, ErrorCode};
use crate::model::{
    ChartOptions, ChartRequest, MAX_MARGIN_MM, MAX_NODES, MAX_NODE_WIDTH_MM, MIN_NODE_WIDTH_MM,
    RATIO_MAX, RATIO_MIN,
};

/// User-supplied node ids must not use this prefix. Layout assigns these ids later.
pub(crate) const OVERFLOW_ID_PREFIX: &str = "__overflow__:";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Visit {
    Unvisited,
    Visiting,
    Visited,
}

struct Endpoint {
    from: usize,
    to: usize,
}

/// A request that passed [`validate`].
///
/// Nodes and edges are addressed by index into `request`. `outgoing` and
/// `incoming` list edge indexes. `out_nodes` lists the head of each outgoing edge.
pub(crate) struct ValidatedGraph<'a> {
    pub(crate) request: &'a ChartRequest,
    index_of: HashMap<&'a str, usize>,
    ends: Vec<Endpoint>,
    outgoing: Vec<Vec<usize>>,
    incoming: Vec<Vec<usize>>,
    out_nodes: Vec<Vec<usize>>,
    pub(crate) subject: usize,
}

impl<'a> ValidatedGraph<'a> {
    pub(crate) fn node_id(&self, index: usize) -> &'a str {
        &self.request.nodes[index].id
    }

    pub(crate) fn contains_id(&self, id: &str) -> bool {
        self.index_of.contains_key(id)
    }

    pub(crate) fn incoming_edges(&self, node: usize) -> &[usize] {
        &self.incoming[node]
    }

    pub(crate) fn edge_from(&self, edge: usize) -> usize {
        self.ends[edge].from
    }

    pub(crate) fn edge_to(&self, edge: usize) -> usize {
        self.ends[edge].to
    }
}

/// Check `request` and index it. Folding is a separate step.
pub(crate) fn validate(request: &ChartRequest) -> Result<ValidatedGraph<'_>, Error> {
    if request.subject_id.trim().is_empty() {
        return Err(Error::new(
            ErrorCode::InvalidProtocol,
            "subject_id is blank",
        ));
    }
    let max_nodes = option_bounds(&request.options)?;
    if request.nodes.len() > max_nodes {
        return Err(Error::new(
            ErrorCode::TooManyNodes,
            format!(
                "nodes={} exceeds max_nodes={max_nodes}",
                request.nodes.len()
            ),
        ));
    }
    let index_of = index_nodes(request)?;
    let Some(&subject) = index_of.get(request.subject_id.as_str()) else {
        return Err(Error::new(
            ErrorCode::SubjectNotFound,
            format!("subject_id={} not in nodes", request.subject_id),
        ));
    };
    let ends = check_edges(request, &index_of)?;
    let node_count = request.nodes.len();
    let mut outgoing = vec![Vec::new(); node_count];
    let mut incoming = vec![Vec::new(); node_count];
    let mut out_nodes = vec![Vec::new(); node_count];
    for (edge_index, end) in ends.iter().enumerate() {
        outgoing[end.from].push(edge_index);
        incoming[end.to].push(edge_index);
        out_nodes[end.from].push(end.to);
    }
    detect_cycle(&out_nodes, request)?;
    let graph = ValidatedGraph {
        request,
        index_of,
        ends,
        outgoing,
        incoming,
        out_nodes,
        subject,
    };
    require_dual_cone(&graph)?;
    reject_ambiguous_ratios(&graph)?;
    Ok(graph)
}

fn option_bounds(options: &ChartOptions) -> Result<usize, Error> {
    if let Some(max_nodes) = options.max_nodes {
        if !(1..=MAX_NODES).contains(&max_nodes) {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("max_nodes={max_nodes} exceeds limit {MAX_NODES}"),
            ));
        }
    }
    let max_nodes = options.max_nodes.unwrap_or(MAX_NODES);
    if let Some(top_n) = options.top_n {
        if top_n > max_nodes {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("top_n={top_n} must be in 0..{max_nodes}"),
            ));
        }
    }
    if let Some(margin_mm) = options.margin_mm {
        if !(margin_mm.is_finite() && margin_mm > 0.0 && margin_mm <= MAX_MARGIN_MM) {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("margin_mm={margin_mm:?} must be in (0, {MAX_MARGIN_MM}]"),
            ));
        }
    }
    if let Some(node_width_mm) = options.node_width_mm {
        let in_range = node_width_mm.is_finite()
            && (MIN_NODE_WIDTH_MM..=MAX_NODE_WIDTH_MM).contains(&node_width_mm);
        if !in_range {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!(
                    "node_width_mm={node_width_mm:?} must be in [{MIN_NODE_WIDTH_MM}, {MAX_NODE_WIDTH_MM}]"
                ),
            ));
        }
    }
    Ok(max_nodes)
}

fn index_nodes(request: &ChartRequest) -> Result<HashMap<&str, usize>, Error> {
    let mut index_of = HashMap::with_capacity(request.nodes.len());
    for (index, node) in request.nodes.iter().enumerate() {
        if node.id.trim().is_empty() || node.name.trim().is_empty() {
            return Err(Error::new(ErrorCode::InvalidProtocol, "node id/name blank"));
        }
        if index_of.contains_key(node.id.as_str()) {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("duplicate node id={}", node.id),
            ));
        }
        if node.id.starts_with(OVERFLOW_ID_PREFIX) {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!(
                    "node id uses reserved prefix {OVERFLOW_ID_PREFIX}: {}",
                    node.id
                ),
            ));
        }
        index_of.insert(node.id.as_str(), index);
    }
    Ok(index_of)
}

fn check_edges(
    request: &ChartRequest,
    index_of: &HashMap<&str, usize>,
) -> Result<Vec<Endpoint>, Error> {
    let mut seen = HashSet::with_capacity(request.edges.len());
    let mut ends = Vec::with_capacity(request.edges.len());
    for edge in &request.edges {
        let (Some(&from), Some(&to)) = (
            index_of.get(edge.from_id.as_str()),
            index_of.get(edge.to_id.as_str()),
        ) else {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("dangling edge {}->{}", edge.from_id, edge.to_id),
            ));
        };
        if from == to {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("self-loop on {}", edge.from_id),
            ));
        }
        if !seen.insert((from, to)) {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("duplicate edge {}->{}", edge.from_id, edge.to_id),
            ));
        }
        require_ratio(
            edge.ratio,
            &format!("edge {}->{}", edge.from_id, edge.to_id),
        )?;
        ends.push(Endpoint { from, to });
    }
    for node in &request.nodes {
        require_ratio(node.ratio, &format!("node {}", node.id))?;
    }
    Ok(ends)
}

fn require_ratio(value: Option<f64>, label: &str) -> Result<(), Error> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.is_finite() && (RATIO_MIN..=RATIO_MAX).contains(&value) {
        return Ok(());
    }
    Err(Error::new(
        ErrorCode::InvalidProtocol,
        format!("{label} ratio={value:?} must be finite and in {RATIO_MIN}..{RATIO_MAX}"),
    ))
}

fn detect_cycle(out_nodes: &[Vec<usize>], request: &ChartRequest) -> Result<(), Error> {
    let mut state = vec![Visit::Unvisited; out_nodes.len()];
    for start in 0..out_nodes.len() {
        if state[start] != Visit::Unvisited {
            continue;
        }
        let mut stack = vec![(start, 0usize)];
        state[start] = Visit::Visiting;
        while let Some(&(node, mut child_index)) = stack.last() {
            if child_index < out_nodes[node].len() {
                let target = out_nodes[node][child_index];
                child_index += 1;
                if let Some(top) = stack.last_mut() {
                    top.1 = child_index;
                }
                match state[target] {
                    Visit::Visiting => {
                        return Err(Error::new(
                            ErrorCode::CycleDetected,
                            format!(
                                "cycle at {}->{}",
                                request.nodes[node].id, request.nodes[target].id
                            ),
                        ));
                    }
                    Visit::Unvisited => {
                        state[target] = Visit::Visiting;
                        stack.push((target, 0));
                    }
                    Visit::Visited => {}
                }
            } else {
                stack.pop();
                state[node] = Visit::Visited;
            }
        }
    }
    Ok(())
}

/// Shareholder cone, excluding the subject.
pub(crate) fn shareholder_ids(graph: &ValidatedGraph<'_>) -> HashSet<usize> {
    walk_cone(graph, Cone::Shareholder)
}

/// Investment cone, excluding the subject.
pub(crate) fn investment_ids(graph: &ValidatedGraph<'_>) -> HashSet<usize> {
    walk_cone(graph, Cone::Investment)
}

#[derive(Clone, Copy)]
enum Cone {
    Shareholder,
    Investment,
}

fn walk_cone(graph: &ValidatedGraph<'_>, cone: Cone) -> HashSet<usize> {
    let mut into = HashSet::new();
    let mut stack = vec![graph.subject];
    let mut visited = HashSet::from([graph.subject]);
    while let Some(current) = stack.pop() {
        let nexts: Vec<usize> = match cone {
            Cone::Shareholder => graph.incoming[current]
                .iter()
                .map(|&edge| graph.ends[edge].from)
                .collect(),
            Cone::Investment => graph.out_nodes[current].clone(),
        };
        for next in nexts {
            if visited.insert(next) {
                into.insert(next);
                stack.push(next);
            }
        }
    }
    into
}

fn require_dual_cone(graph: &ValidatedGraph<'_>) -> Result<(), Error> {
    let shareholders = shareholder_ids(graph);
    let investments = investment_ids(graph);
    let mut orphans = Vec::new();
    for index in 0..graph.request.nodes.len() {
        if index != graph.subject && !shareholders.contains(&index) && !investments.contains(&index)
        {
            orphans.push(graph.node_id(index));
        }
    }
    if !orphans.is_empty() {
        orphans.sort_unstable();
        return Err(Error::new(
            ErrorCode::InvalidProtocol,
            format!(
                "nodes not connected to subject={}: {}",
                graph.request.subject_id,
                orphans.join(",")
            ),
        ));
    }
    for (edge_index, edge) in graph.request.edges.iter().enumerate() {
        let from = graph.ends[edge_index].from;
        let to = graph.ends[edge_index].to;
        let up =
            shareholders.contains(&from) && (shareholders.contains(&to) || to == graph.subject);
        let down =
            investments.contains(&to) && (from == graph.subject || investments.contains(&from));
        // A back edge into the shareholder cone is already a cycle.
        // What remains is a shareholder that also holds an investment.
        if !up && !down {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!(
                    "edge crosses shareholder/investment cones: {}->{}",
                    edge.from_id, edge.to_id
                ),
            ));
        }
    }
    Ok(())
}

fn reject_ambiguous_ratios(graph: &ValidatedGraph<'_>) -> Result<(), Error> {
    let mut cache = HashMap::new();
    for (index, node) in graph.request.nodes.iter().enumerate() {
        if node.ratio.is_some() || index == graph.subject {
            continue;
        }
        unique_layout_parent_edge(graph, index, &mut cache)?;
    }
    Ok(())
}

fn unique_layout_parent_edge(
    graph: &ValidatedGraph<'_>,
    node: usize,
    cache: &mut HashMap<(usize, usize), bool>,
) -> Result<Option<usize>, Error> {
    if node == graph.subject {
        return Ok(None);
    }
    let to_subject: Vec<usize> = graph.outgoing[node]
        .iter()
        .copied()
        .filter(|&edge| graph.ends[edge].to == graph.subject)
        .collect();
    if !to_subject.is_empty() {
        return single_edge(graph, &to_subject, node).map(Some);
    }
    let from_subject: Vec<usize> = graph.incoming[node]
        .iter()
        .copied()
        .filter(|&edge| graph.ends[edge].from == graph.subject)
        .collect();
    if !from_subject.is_empty() {
        return single_edge(graph, &from_subject, node).map(Some);
    }
    let down: Vec<usize> = graph.outgoing[node]
        .iter()
        .copied()
        .filter(|&edge| reaches(graph, graph.ends[edge].to, graph.subject, cache))
        .collect();
    if !down.is_empty() {
        return single_edge(graph, &down, node).map(Some);
    }
    let up: Vec<usize> = graph.incoming[node]
        .iter()
        .copied()
        .filter(|&edge| {
            let from = graph.ends[edge].from;
            from == graph.subject || reaches(graph, graph.subject, from, cache)
        })
        .collect();
    if !up.is_empty() {
        return single_edge(graph, &up, node).map(Some);
    }
    Ok(None)
}

fn single_edge(graph: &ValidatedGraph<'_>, edges: &[usize], node: usize) -> Result<usize, Error> {
    if let [only] = edges {
        Ok(*only)
    } else {
        Err(ambiguous(graph, edges, node))
    }
}

fn ambiguous(graph: &ValidatedGraph<'_>, edges: &[usize], node: usize) -> Error {
    let ratios: Vec<Option<f64>> = edges
        .iter()
        .map(|&edge| graph.request.edges[edge].ratio)
        .collect();
    Error::new(
        ErrorCode::InvalidProtocol,
        format!(
            "ambiguous parent edges for {} count={} ratios={ratios:?}",
            graph.node_id(node),
            edges.len()
        ),
    )
}

fn reaches(
    graph: &ValidatedGraph<'_>,
    from: usize,
    target: usize,
    cache: &mut HashMap<(usize, usize), bool>,
) -> bool {
    if let Some(found) = cache.get(&(from, target)) {
        return *found;
    }
    if from == target {
        cache.insert((from, target), true);
        return true;
    }
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([from]);
    let mut found = false;
    while let Some(current) = queue.pop_front() {
        if !seen.insert(current) {
            continue;
        }
        for &next in &graph.out_nodes[current] {
            if next == target {
                found = true;
                break;
            }
            queue.push_back(next);
        }
        if found {
            break;
        }
    }
    cache.insert((from, target), found);
    found
}

/// Display percent for `node`.
///
/// A ratio stored on the node wins, and a diamond then does not need one parent.
/// The subject has no display ratio. Otherwise the unique layout parent edge
/// supplies the percent, or `None` when that edge has no ratio.
pub(crate) fn resolve_display_ratio(
    graph: &ValidatedGraph<'_>,
    node: usize,
    cache: &mut HashMap<(usize, usize), bool>,
) -> Result<Option<f64>, Error> {
    if let Some(ratio) = graph.request.nodes[node].ratio {
        return Ok(Some(ratio));
    }
    if node == graph.subject {
        return Ok(None);
    }
    let parent = unique_layout_parent_edge(graph, node, cache)?;
    Ok(parent.and_then(|edge| graph.request.edges[edge].ratio))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use super::{investment_ids, resolve_display_ratio, shareholder_ids, validate};
    use crate::model::{ChartRequest, Edge, Node};

    fn index_of(request: &ChartRequest, id: &str) -> usize {
        request
            .nodes
            .iter()
            .position(|node| node.id == id)
            .expect("node id")
    }

    fn ratio_of(request: &ChartRequest, id: &str) -> Option<f64> {
        let graph = validate(request).expect("valid graph");
        let mut cache = HashMap::new();
        resolve_display_ratio(&graph, index_of(request, id), &mut cache).expect("ratio")
    }

    #[test]
    fn node_ratio_wins_over_the_edge() {
        let request = ChartRequest::new(
            "root",
            vec![
                Node {
                    ratio: Some(55.0),
                    ..Node::new("a", "甲")
                },
                Node::new("root", "主体"),
            ],
            vec![Edge {
                ratio: Some(60.0),
                ..Edge::new("a", "root")
            }],
        );
        assert_eq!(ratio_of(&request, "a"), Some(55.0));
    }

    #[test]
    fn backfills_from_the_unique_edge() {
        let request = ChartRequest::new(
            "root",
            vec![Node::new("a", "甲"), Node::new("root", "主体")],
            vec![Edge {
                ratio: Some(60.0),
                ..Edge::new("a", "root")
            }],
        );
        assert_eq!(ratio_of(&request, "a"), Some(60.0));
    }

    #[test]
    fn missing_ratio_is_none() {
        let request = ChartRequest::new(
            "root",
            vec![Node::new("a", "甲"), Node::new("root", "主体")],
            vec![Edge::new("a", "root")],
        );
        assert_eq!(ratio_of(&request, "a"), None);
    }

    #[test]
    fn diamond_with_a_node_ratio_uses_that_ratio() {
        let request = ChartRequest::new(
            "root",
            vec![
                Node::new("root", "主体"),
                Node::new("a", "甲"),
                Node::new("b", "乙"),
                Node {
                    ratio: Some(50.0),
                    ..Node::new("d", "合资")
                },
            ],
            vec![
                Edge::new("root", "a"),
                Edge::new("root", "b"),
                Edge::new("a", "d"),
                Edge::new("b", "d"),
            ],
        );
        assert_eq!(ratio_of(&request, "d"), Some(50.0));
        assert_eq!(ratio_of(&request, "root"), None);
    }

    #[test]
    fn subject_does_not_take_a_holder_edge_ratio() {
        let request = ChartRequest::new(
            "root",
            vec![Node::new("root", "主体"), Node::new("a", "甲")],
            vec![Edge {
                ratio: Some(60.0),
                ..Edge::new("a", "root")
            }],
        );
        assert_eq!(ratio_of(&request, "root"), None);
    }

    #[test]
    fn horizontal_edges_stay_inside_one_cone() {
        let request = ChartRequest::new(
            "root",
            vec![
                Node::new("a", "a"),
                Node::new("b", "b"),
                Node::new("root", "root"),
                Node::new("x", "x"),
                Node::new("y", "y"),
            ],
            vec![
                Edge::new("a", "root"),
                Edge::new("b", "root"),
                Edge::new("a", "b"),
                Edge::new("root", "x"),
                Edge::new("root", "y"),
                Edge::new("x", "y"),
            ],
        );
        let graph = validate(&request).expect("valid graph");
        let holders: BTreeSet<_> = shareholder_ids(&graph)
            .into_iter()
            .map(|index| request.nodes[index].id.as_str())
            .collect();
        let investments: BTreeSet<_> = investment_ids(&graph)
            .into_iter()
            .map(|index| request.nodes[index].id.as_str())
            .collect();
        assert_eq!(holders, BTreeSet::from(["a", "b"]));
        assert_eq!(investments, BTreeSet::from(["x", "y"]));
    }
}
