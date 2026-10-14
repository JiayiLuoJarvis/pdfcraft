//! Protocol checks for one chart: ids, edges, cycles, the two cones, and parent ratios.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::{Error, ErrorCode};
use crate::ids::{EdgeId, EdgeRec, NodeId, NodeOrigin, NodeRec};
use crate::model::{
    ChartOptions, ChartRequest, MAX_MARGIN_MM, MAX_NODES, MAX_NODE_WIDTH_MM, MIN_NODE_WIDTH_MM,
    RATIO_MAX, RATIO_MIN,
};

/// User-supplied node ids must not use this prefix. Layout assigns these ids later.
pub(crate) const OVERFLOW_ID_PREFIX: &str = "__overflow__:";

/// Id of the stand-in for direct children moved off one page.
///
/// `first_moved_id` is the earliest moved child in input order, so one subject
/// can overflow more than once.
#[must_use]
pub(crate) fn overflow_node_id(subject_id: &str, first_moved_id: &str) -> String {
    format!("{OVERFLOW_ID_PREFIX}{subject_id}:{first_moved_id}")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Visit {
    Unvisited,
    Visiting,
    Visited,
}

/// A request that passed [`validate`], owned as an arena.
///
/// Nodes and edges are addressed by [`NodeId`] / [`EdgeId`]. `outgoing` and
/// `incoming` are indexed by [`NodeId::index`].
#[derive(Debug, Clone)]
pub(crate) struct Graph {
    pub(crate) nodes: Vec<NodeRec>,
    pub(crate) edges: Vec<EdgeRec>,
    by_protocol_id: HashMap<String, NodeId>,
    outgoing: Vec<Vec<EdgeId>>,
    incoming: Vec<Vec<EdgeId>>,
    out_nodes: Vec<Vec<NodeId>>,
    pub(crate) subject: NodeId,
    pub(crate) subject_protocol_id: String,
    pub(crate) options: ChartOptions,
    /// Reachability answers for layout-parent selection. Same lifetime as the graph.
    reach_cache: HashMap<(NodeId, NodeId), bool>,
    /// Overflow boxes already appended for `(subject, first_moved)`.
    overflow_ids: HashMap<(NodeId, NodeId), NodeId>,
}

impl Graph {
    pub(crate) fn protocol_id(&self, id: NodeId) -> &str {
        &self.nodes[id.index()].protocol_id
    }

    pub(crate) fn node_name(&self, id: NodeId) -> &str {
        &self.nodes[id.index()].name
    }

    pub(crate) fn contains_protocol(&self, protocol: &str) -> bool {
        self.by_protocol_id.contains_key(protocol)
    }

    #[cfg(test)]
    pub(crate) fn id_of(&self, protocol: &str) -> Option<NodeId> {
        self.by_protocol_id.get(protocol).copied()
    }

    pub(crate) fn incoming_edges(&self, node: NodeId) -> &[EdgeId] {
        &self.incoming[node.index()]
    }

    pub(crate) fn edge_from(&self, edge: EdgeId) -> NodeId {
        self.edges[edge.index()].from
    }

    pub(crate) fn edge_to(&self, edge: EdgeId) -> NodeId {
        self.edges[edge.index()].to
    }

    /// Holders of `id` (`from` on an incoming edge), in edge order.
    pub(crate) fn incoming_sources(&self, id: NodeId) -> Vec<NodeId> {
        self.incoming[id.index()]
            .iter()
            .map(|&edge| self.edge_from(edge))
            .collect()
    }

    /// Nodes `id` holds (`to` on an outgoing edge), in edge order.
    pub(crate) fn outgoing_targets(&self, id: NodeId) -> Vec<NodeId> {
        self.outgoing[id.index()]
            .iter()
            .map(|&edge| self.edge_to(edge))
            .collect()
    }

    /// Endpoints of the unique layout-parent edge, or `None` when there is no parent.
    ///
    /// An ambiguous parent is [`ErrorCode::InvalidProtocol`].
    pub(crate) fn layout_parent_ends(
        &mut self,
        id: NodeId,
    ) -> Result<Option<(NodeId, NodeId)>, Error> {
        let Some(edge) = unique_layout_parent_edge(self, id)? else {
            return Ok(None);
        };
        Ok(Some((self.edge_from(edge), self.edge_to(edge))))
    }

    /// Unique layout parent when one exists. No parent or an ambiguous parent is `None`.
    pub(crate) fn layout_parent_optional(&mut self, id: NodeId) -> Option<(NodeId, NodeId)> {
        self.layout_parent_ends(id).unwrap_or_default()
    }

    /// Append an overflow node, or reuse the id for the same subject and first moved child.
    pub(crate) fn ensure_overflow(
        &mut self,
        subject: NodeId,
        first_moved: NodeId,
        moved_count: usize,
        name: String,
    ) -> NodeId {
        if let Some(id) = self.overflow_ids.get(&(subject, first_moved)) {
            return *id;
        }
        let protocol = overflow_node_id(self.protocol_id(subject), self.protocol_id(first_moved));
        let id = NodeId::from_index(self.nodes.len());
        self.nodes.push(NodeRec {
            id,
            origin: NodeOrigin::Overflow {
                subject,
                first_moved,
                moved_count,
            },
            protocol_id: protocol.clone(),
            name,
            legal_person: None,
            ratio: None,
            node_type: None,
            paid_in_amount: None,
            currency: None,
            location: None,
            approval_date: None,
        });
        self.by_protocol_id.insert(protocol, id);
        self.outgoing.push(Vec::new());
        self.incoming.push(Vec::new());
        self.out_nodes.push(Vec::new());
        self.overflow_ids.insert((subject, first_moved), id);
        id
    }

    /// Rebuild a [`ChartRequest`] for the prepare boundary (fold output).
    pub(crate) fn to_request(&self) -> ChartRequest {
        ChartRequest {
            subject_id: self.subject_protocol_id.clone(),
            nodes: self
                .nodes
                .iter()
                .filter(|node| !matches!(node.origin, NodeOrigin::Overflow { .. }))
                .map(|node| crate::model::Node {
                    id: node.protocol_id.clone(),
                    name: node.name.clone(),
                    legal_person: node.legal_person.clone(),
                    ratio: node.ratio,
                    node_type: node.node_type,
                    paid_in_amount: node.paid_in_amount.clone(),
                    currency: node.currency.clone(),
                    location: node.location.clone(),
                    approval_date: node.approval_date.clone(),
                })
                .collect(),
            edges: self
                .edges
                .iter()
                .map(|edge| crate::model::Edge {
                    from_id: self.protocol_id(edge.from).to_owned(),
                    to_id: self.protocol_id(edge.to).to_owned(),
                    ratio: edge.ratio,
                })
                .collect(),
            options: self.options.clone(),
        }
    }
}

/// Check `request` and copy it into an arena. Folding is a separate step.
pub(crate) fn validate(request: &ChartRequest) -> Result<Graph, Error> {
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
    let (nodes, by_protocol_id) = index_nodes(request)?;
    let Some(&subject) = by_protocol_id.get(request.subject_id.as_str()) else {
        return Err(Error::new(
            ErrorCode::SubjectNotFound,
            format!("subject_id={} not in nodes", request.subject_id),
        ));
    };
    let edges = check_edges(request, &by_protocol_id)?;
    let node_count = nodes.len();
    let mut outgoing = vec![Vec::new(); node_count];
    let mut incoming = vec![Vec::new(); node_count];
    let mut out_nodes = vec![Vec::new(); node_count];
    for edge in &edges {
        outgoing[edge.from.index()].push(edge.id);
        incoming[edge.to.index()].push(edge.id);
        out_nodes[edge.from.index()].push(edge.to);
    }
    detect_cycle(&out_nodes, &nodes)?;
    let mut graph = Graph {
        nodes,
        edges,
        by_protocol_id,
        outgoing,
        incoming,
        out_nodes,
        subject,
        subject_protocol_id: request.subject_id.clone(),
        options: request.options.clone(),
        reach_cache: HashMap::new(),
        overflow_ids: HashMap::new(),
    };
    require_dual_cone(&graph)?;
    reject_ambiguous_ratios(&mut graph)?;
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

fn index_nodes(request: &ChartRequest) -> Result<(Vec<NodeRec>, HashMap<String, NodeId>), Error> {
    let mut by_protocol_id = HashMap::with_capacity(request.nodes.len());
    let mut nodes = Vec::with_capacity(request.nodes.len());
    for (index, node) in request.nodes.iter().enumerate() {
        if node.id.trim().is_empty() || node.name.trim().is_empty() {
            return Err(Error::new(ErrorCode::InvalidProtocol, "node id/name blank"));
        }
        if by_protocol_id.contains_key(node.id.as_str()) {
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
        let id = NodeId::from_index(index);
        let origin = folded_origin(&node.id, &by_protocol_id).unwrap_or(NodeOrigin::Input);
        by_protocol_id.insert(node.id.clone(), id);
        nodes.push(NodeRec {
            id,
            origin,
            protocol_id: node.id.clone(),
            name: node.name.clone(),
            legal_person: node.legal_person.clone(),
            ratio: node.ratio,
            node_type: node.node_type,
            paid_in_amount: node.paid_in_amount.clone(),
            currency: node.currency.clone(),
            location: node.location.clone(),
            approval_date: node.approval_date.clone(),
        });
    }
    Ok((nodes, by_protocol_id))
}

/// When a later node is `__other__:{parent}` and that parent is already indexed,
/// treat it as a fold stand-in. Caller-written `__other__:` ids whose parent is
/// missing stay [`NodeOrigin::Input`].
fn folded_origin(
    protocol_id: &str,
    by_protocol_id: &HashMap<String, NodeId>,
) -> Option<NodeOrigin> {
    let parent_protocol = protocol_id.strip_prefix("__other__:")?;
    let parent = *by_protocol_id.get(parent_protocol)?;
    Some(NodeOrigin::FoldedHolders { parent })
}

fn check_edges(
    request: &ChartRequest,
    by_protocol_id: &HashMap<String, NodeId>,
) -> Result<Vec<EdgeRec>, Error> {
    let mut seen = HashSet::with_capacity(request.edges.len());
    let mut edges = Vec::with_capacity(request.edges.len());
    for (index, edge) in request.edges.iter().enumerate() {
        let (Some(&from), Some(&to)) = (
            by_protocol_id.get(edge.from_id.as_str()),
            by_protocol_id.get(edge.to_id.as_str()),
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
        edges.push(EdgeRec {
            id: EdgeId::from_index(index),
            from,
            to,
            ratio: edge.ratio,
        });
    }
    for node in &request.nodes {
        require_ratio(node.ratio, &format!("node {}", node.id))?;
    }
    Ok(edges)
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

fn detect_cycle(out_nodes: &[Vec<NodeId>], nodes: &[NodeRec]) -> Result<(), Error> {
    let mut state = vec![Visit::Unvisited; out_nodes.len()];
    for start in 0..out_nodes.len() {
        if state[start] != Visit::Unvisited {
            continue;
        }
        let mut stack = vec![(start, 0usize)];
        state[start] = Visit::Visiting;
        while let Some(&(node, mut child_index)) = stack.last() {
            if child_index < out_nodes[node].len() {
                let target = out_nodes[node][child_index].index();
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
                                nodes[node].protocol_id, nodes[target].protocol_id
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
pub(crate) fn shareholder_ids(graph: &Graph) -> HashSet<NodeId> {
    walk_cone(graph, Cone::Shareholder)
}

/// Investment cone, excluding the subject.
pub(crate) fn investment_ids(graph: &Graph) -> HashSet<NodeId> {
    walk_cone(graph, Cone::Investment)
}

#[derive(Clone, Copy)]
enum Cone {
    Shareholder,
    Investment,
}

fn walk_cone(graph: &Graph, cone: Cone) -> HashSet<NodeId> {
    let mut into = HashSet::new();
    let mut stack = vec![graph.subject];
    let mut visited = HashSet::from([graph.subject]);
    while let Some(current) = stack.pop() {
        let nexts: Vec<NodeId> = match cone {
            Cone::Shareholder => graph.incoming_sources(current),
            Cone::Investment => graph.out_nodes[current.index()].clone(),
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

fn require_dual_cone(graph: &Graph) -> Result<(), Error> {
    let shareholders = shareholder_ids(graph);
    let investments = investment_ids(graph);
    let mut orphans = Vec::new();
    for node in &graph.nodes {
        if node.id != graph.subject
            && !shareholders.contains(&node.id)
            && !investments.contains(&node.id)
        {
            orphans.push(node.protocol_id.as_str());
        }
    }
    if !orphans.is_empty() {
        orphans.sort_unstable();
        return Err(Error::new(
            ErrorCode::InvalidProtocol,
            format!(
                "nodes not connected to subject={}: {}",
                graph.subject_protocol_id,
                orphans.join(",")
            ),
        ));
    }
    for edge in &graph.edges {
        let from = edge.from;
        let to = edge.to;
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
                    graph.protocol_id(from),
                    graph.protocol_id(to)
                ),
            ));
        }
    }
    Ok(())
}

fn reject_ambiguous_ratios(graph: &mut Graph) -> Result<(), Error> {
    for index in 0..graph.nodes.len() {
        let id = NodeId::from_index(index);
        if graph.nodes[index].ratio.is_some() || id == graph.subject {
            continue;
        }
        unique_layout_parent_edge(graph, id)?;
    }
    Ok(())
}

pub(crate) fn unique_layout_parent_edge(
    graph: &mut Graph,
    node: NodeId,
) -> Result<Option<EdgeId>, Error> {
    if node == graph.subject {
        return Ok(None);
    }
    let to_subject: Vec<EdgeId> = graph.outgoing[node.index()]
        .iter()
        .copied()
        .filter(|&edge| graph.edge_to(edge) == graph.subject)
        .collect();
    if !to_subject.is_empty() {
        return single_edge(graph, &to_subject, node).map(Some);
    }
    let from_subject: Vec<EdgeId> = graph.incoming[node.index()]
        .iter()
        .copied()
        .filter(|&edge| graph.edge_from(edge) == graph.subject)
        .collect();
    if !from_subject.is_empty() {
        return single_edge(graph, &from_subject, node).map(Some);
    }
    let outgoing = graph.outgoing[node.index()].clone();
    let mut down = Vec::new();
    for edge in outgoing {
        let to = graph.edge_to(edge);
        if reaches(graph, to, graph.subject) {
            down.push(edge);
        }
    }
    if !down.is_empty() {
        return single_edge(graph, &down, node).map(Some);
    }
    let incoming = graph.incoming[node.index()].clone();
    let mut up = Vec::new();
    for edge in incoming {
        let from = graph.edge_from(edge);
        if from == graph.subject || reaches(graph, graph.subject, from) {
            up.push(edge);
        }
    }
    if !up.is_empty() {
        return single_edge(graph, &up, node).map(Some);
    }
    Ok(None)
}

fn single_edge(graph: &Graph, edges: &[EdgeId], node: NodeId) -> Result<EdgeId, Error> {
    if let [only] = edges {
        Ok(*only)
    } else {
        Err(ambiguous(graph, edges, node))
    }
}

fn ambiguous(graph: &Graph, edges: &[EdgeId], node: NodeId) -> Error {
    let ratios: Vec<Option<f64>> = edges
        .iter()
        .map(|&edge| graph.edges[edge.index()].ratio)
        .collect();
    Error::new(
        ErrorCode::InvalidProtocol,
        format!(
            "ambiguous parent edges for {} count={} ratios={ratios:?}",
            graph.protocol_id(node),
            edges.len()
        ),
    )
}

fn reaches(graph: &mut Graph, from: NodeId, target: NodeId) -> bool {
    if let Some(found) = graph.reach_cache.get(&(from, target)) {
        return *found;
    }
    if from == target {
        graph.reach_cache.insert((from, target), true);
        return true;
    }
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([from]);
    let mut found = false;
    while let Some(current) = queue.pop_front() {
        if !seen.insert(current) {
            continue;
        }
        for &next in &graph.out_nodes[current.index()] {
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
    graph.reach_cache.insert((from, target), found);
    found
}

/// Where a display percent came from. `parent` is set only when the value is an edge ratio.
pub(crate) struct RatioSource {
    pub(crate) value: f64,
    pub(crate) parent: Option<EdgeId>,
}

/// Display percent for `node`, plus the parent edge when the percent came from that edge.
///
/// A ratio stored on the node wins, and a diamond then does not need one parent.
/// The subject has no display ratio. Otherwise the unique layout parent edge
/// supplies the percent, or `None` when that edge has no ratio.
pub(crate) fn ratio_source(graph: &mut Graph, node: NodeId) -> Result<Option<RatioSource>, Error> {
    if let Some(ratio) = graph.nodes[node.index()].ratio {
        return Ok(Some(RatioSource {
            value: ratio,
            parent: None,
        }));
    }
    if node == graph.subject {
        return Ok(None);
    }
    let Some(parent) = unique_layout_parent_edge(graph, node)? else {
        return Ok(None);
    };
    let Some(value) = graph.edges[parent.index()].ratio else {
        return Ok(None);
    };
    Ok(Some(RatioSource {
        value,
        parent: Some(parent),
    }))
}

/// Display percent for `node`. See [`ratio_source`].
pub(crate) fn resolve_display_ratio(graph: &mut Graph, node: NodeId) -> Result<Option<f64>, Error> {
    Ok(ratio_source(graph, node)?.map(|source| source.value))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{investment_ids, resolve_display_ratio, shareholder_ids, validate};
    use crate::model::{ChartRequest, Edge, Node};

    fn id_of(graph: &super::Graph, protocol: &str) -> crate::ids::NodeId {
        graph.id_of(protocol).expect("node id")
    }

    fn ratio_of(request: &ChartRequest, protocol: &str) -> Option<f64> {
        let mut graph = validate(request).expect("valid graph");
        let id = id_of(&graph, protocol);
        resolve_display_ratio(&mut graph, id).expect("ratio")
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
            .map(|id| graph.protocol_id(id))
            .collect();
        let investments: BTreeSet<_> = investment_ids(&graph)
            .into_iter()
            .map(|id| graph.protocol_id(id))
            .collect();
        assert_eq!(holders, BTreeSet::from(["a", "b"]));
        assert_eq!(investments, BTreeSet::from(["x", "y"]));
    }
}
