//! Directional focus movement across terminal window leaves.

use super::TerminalWindowNode;
use crate::workspace_layout::pane::WorkspaceSplitDirection;

/// Direction a directional pane-focus command moves toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneNavigationDirection {
    Left,
    Right,
    Up,
    Down,
}

impl PaneNavigationDirection {
    /// A split on the opposite axis never contributes a neighbour: the two
    /// subtrees overlap on the navigation axis, so crossing would move into a
    /// pane that is visually below or above rather than beside the current one.
    fn follows_split(self, split: WorkspaceSplitDirection) -> bool {
        matches!(
            (self, split),
            (Self::Left | Self::Right, WorkspaceSplitDirection::Vertical)
                | (Self::Up | Self::Down, WorkspaceSplitDirection::Horizontal)
        )
    }

    /// Whether the target subtree is the split's `second` child.
    fn enters_second(self) -> bool {
        matches!(self, Self::Right | Self::Down)
    }

    /// Edge of the target subtree that borders the current leaf.
    fn entry_edge(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
}

/// Result of searching one subtree for the current leaf and its neighbour.
enum LeafSearch {
    Found(String),
    /// The current leaf is inside this subtree but no neighbour exists here.
    Boundary,
}

impl TerminalWindowNode {
    /// Leaf directly adjacent to `current` in `direction`.
    ///
    /// Returns `None` when the leaf is on that edge of the layout.
    pub fn adjacent_leaf(
        &self,
        current: &str,
        direction: PaneNavigationDirection,
    ) -> Option<String> {
        match self.search_leaf(current, direction)? {
            LeafSearch::Found(id) => Some(id),
            LeafSearch::Boundary => None,
        }
    }

    fn search_leaf(&self, current: &str, direction: PaneNavigationDirection) -> Option<LeafSearch> {
        match self {
            Self::Leaf { id, .. } => (id == current).then_some(LeafSearch::Boundary),
            Self::Split {
                direction: split_direction,
                first,
                second,
                ..
            } => {
                if let Some(result) = first.search_leaf(current, direction) {
                    return Some(match result {
                        LeafSearch::Found(id) => LeafSearch::Found(id),
                        LeafSearch::Boundary
                            if direction.follows_split(*split_direction)
                                && direction.enters_second() =>
                        {
                            LeafSearch::Found(second.edge_leaf(direction.entry_edge()))
                        }
                        LeafSearch::Boundary => LeafSearch::Boundary,
                    });
                }
                let result = second.search_leaf(current, direction)?;
                Some(match result {
                    LeafSearch::Found(id) => LeafSearch::Found(id),
                    LeafSearch::Boundary
                        if direction.follows_split(*split_direction)
                            && !direction.enters_second() =>
                    {
                        LeafSearch::Found(first.edge_leaf(direction.entry_edge()))
                    }
                    LeafSearch::Boundary => LeafSearch::Boundary,
                })
            }
        }
    }

    /// Outermost leaf on `edge` of this subtree.
    fn edge_leaf(&self, edge: PaneNavigationDirection) -> String {
        match self {
            Self::Leaf { id, .. } => id.clone(),
            Self::Split { first, second, .. } => match edge {
                PaneNavigationDirection::Left | PaneNavigationDirection::Up => {
                    first.edge_leaf(edge)
                }
                PaneNavigationDirection::Right | PaneNavigationDirection::Down => {
                    second.edge_leaf(edge)
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PaneNavigationDirection, TerminalWindowNode, WorkspaceSplitDirection};

    fn leaf(id: &str) -> TerminalWindowNode {
        TerminalWindowNode::Leaf {
            id: id.to_string(),
            tab_ids: vec![id.to_string()],
            active_tab_id: Some(id.to_string()),
        }
    }

    fn split(
        direction: WorkspaceSplitDirection,
        first: TerminalWindowNode,
        second: TerminalWindowNode,
    ) -> TerminalWindowNode {
        TerminalWindowNode::Split {
            id: format!("split-{}", direction.label()),
            direction,
            ratio_percent: 50,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    #[test]
    fn side_by_side_split_navigates_left_and_right() {
        let root = split(WorkspaceSplitDirection::Vertical, leaf("a"), leaf("b"));

        assert_eq!(
            root.adjacent_leaf("a", PaneNavigationDirection::Right)
                .as_deref(),
            Some("b")
        );
        assert_eq!(
            root.adjacent_leaf("b", PaneNavigationDirection::Left)
                .as_deref(),
            Some("a")
        );
        assert_eq!(root.adjacent_leaf("a", PaneNavigationDirection::Left), None);
        assert_eq!(
            root.adjacent_leaf("b", PaneNavigationDirection::Right),
            None
        );
        assert_eq!(root.adjacent_leaf("a", PaneNavigationDirection::Up), None);
        assert_eq!(root.adjacent_leaf("a", PaneNavigationDirection::Down), None);
    }

    #[test]
    fn stacked_split_navigates_up_and_down() {
        let root = split(WorkspaceSplitDirection::Horizontal, leaf("a"), leaf("b"));

        assert_eq!(
            root.adjacent_leaf("a", PaneNavigationDirection::Down)
                .as_deref(),
            Some("b")
        );
        assert_eq!(
            root.adjacent_leaf("b", PaneNavigationDirection::Up)
                .as_deref(),
            Some("a")
        );
        assert_eq!(root.adjacent_leaf("a", PaneNavigationDirection::Up), None);
        assert_eq!(root.adjacent_leaf("b", PaneNavigationDirection::Down), None);
    }

    #[test]
    fn nested_splits_fall_back_to_the_ancestor_split() {
        // a | (b / c): b and c are stacked to the right of a.
        let root = split(
            WorkspaceSplitDirection::Vertical,
            leaf("a"),
            split(WorkspaceSplitDirection::Horizontal, leaf("b"), leaf("c")),
        );

        assert_eq!(
            root.adjacent_leaf("a", PaneNavigationDirection::Right)
                .as_deref(),
            Some("b")
        );
        assert_eq!(
            root.adjacent_leaf("b", PaneNavigationDirection::Left)
                .as_deref(),
            Some("a")
        );
        assert_eq!(
            root.adjacent_leaf("c", PaneNavigationDirection::Left)
                .as_deref(),
            Some("a")
        );
        assert_eq!(
            root.adjacent_leaf("c", PaneNavigationDirection::Up)
                .as_deref(),
            Some("b")
        );
        assert_eq!(
            root.adjacent_leaf("c", PaneNavigationDirection::Right),
            None
        );
    }

    #[test]
    fn opposite_axis_neighbours_do_not_cross_the_split() {
        // (a / b) stacked above c: moving right from a must not jump into c.
        let root = split(
            WorkspaceSplitDirection::Horizontal,
            split(WorkspaceSplitDirection::Vertical, leaf("a"), leaf("b")),
            leaf("c"),
        );

        assert_eq!(
            root.adjacent_leaf("a", PaneNavigationDirection::Right)
                .as_deref(),
            Some("b")
        );
        // Moving up from the full-width bottom row lands on the lower leaf of
        // the top row rather than crossing horizontally.
        assert_eq!(
            root.adjacent_leaf("c", PaneNavigationDirection::Up)
                .as_deref(),
            Some("b")
        );
        assert_eq!(root.adjacent_leaf("c", PaneNavigationDirection::Left), None);
    }

    #[test]
    fn unknown_or_single_leaf_layouts_have_no_neighbour() {
        let root = leaf("a");
        assert_eq!(root.adjacent_leaf("a", PaneNavigationDirection::Left), None);
        assert_eq!(
            root.adjacent_leaf("missing", PaneNavigationDirection::Left),
            None
        );
    }
}
