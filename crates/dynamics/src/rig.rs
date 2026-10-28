//! Compiles an avatar's chains into one flat particle list, parents first, with per-particle parameters (ADR 0015).

use civbb_format::Avatar;
use civbb_format::dynamics::{Chain, Graded, ImmobileType, Integration, MultiChild};
use civbb_format::hierarchy::NodeIndex;
use civbb_format::math::Vec3;

/// Particles per chain at most, as PhysBones limit one component to 256 transforms; deeper nodes are not simulated.
pub const MAX_PARTICLES: usize = 256;

/// Marks a particle without a parent (a chain root).
pub const NO_PARENT: u32 = u32::MAX;

/// One chain's slice of the particle list and its chain-wide settings.
#[derive(Clone, Debug, PartialEq)]
pub struct ChainRig {
    /// The chain's root node; it rotates but never moves.
    pub root: NodeIndex,
    /// The node whose motion `immobile` cancels: the root's parent, `None` for the avatar root.
    pub frame: Option<NodeIndex>,
    /// First particle.
    pub first: u32,
    /// Particle count; the first particle is the root.
    pub count: u32,
    /// Advanced integration (momentum and stiffness).
    pub advanced: bool,
    /// `immobile` cancels only translation (`World`), not rotation too (`AllMotion`).
    pub immobile_world: bool,
    /// How the root aims at several children.
    pub multi_child: MultiChild,
    /// Grabbing allowed.
    pub allow_grab: bool,
    /// Posing allowed.
    pub allow_posing: bool,
}

/// An avatar's chains as structure-of-arrays particles, chains contiguous, every parent before its children.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rig {
    /// Chains, in [`civbb_format::dynamics::Dynamics::chains`] order.
    pub chains: Vec<ChainRig>,
    /// Every node's parent node, to place the animated pose in the world.
    pub parents: Vec<Option<NodeIndex>>,
    /// The node a particle drives; `None` for endpoint tips.
    pub node: Vec<Option<NodeIndex>>,
    /// Parent particle, or [`NO_PARENT`].
    pub parent: Vec<u32>,
    /// Held at the animated pose: the root, and with `MultiChild::Ignore` also the root's children.
    pub pinned: Vec<bool>,
    /// For endpoint tips, the offset in the parent node's space.
    pub endpoint: Vec<Vec3>,
    /// Pull, 0 to 1.
    pub pull: Vec<f32>,
    /// Spring or momentum (velocity kept per reference step), 0 to 1.
    pub spring: Vec<f32>,
    /// Stiffness, 0 to 1 (0 with simplified integration).
    pub stiffness: Vec<f32>,
    /// Gravity, -1 to 1.
    pub gravity: Vec<f32>,
    /// Gravity falloff, 0 to 1.
    pub gravity_falloff: Vec<f32>,
    /// Immobile, 0 to 1.
    pub immobile: Vec<f32>,
    /// Max stretch as a fraction of rest length, 0 or more.
    pub max_stretch: Vec<f32>,
    /// Max squish as a fraction of rest length, 0 to 1.
    pub max_squish: Vec<f32>,
}

impl Rig {
    /// Compiles every chain of a validated avatar.
    pub fn new(avatar: &Avatar) -> Self {
        let nodes = &avatar.hierarchy.nodes;
        let mut children: Vec<Vec<NodeIndex>> = vec![Vec::new(); nodes.len()];
        for (i, n) in nodes.iter().enumerate() {
            if let Some(p) = n.parent {
                children[p as usize].push(i as NodeIndex);
            }
        }
        let mut rig = Self {
            parents: nodes.iter().map(|n| n.parent).collect(),
            ..Self::default()
        };
        for chain in &avatar.dynamics.chains {
            rig.add(chain, &children, nodes[chain.root as usize].parent);
        }
        rig
    }

    /// Number of particles.
    pub fn len(&self) -> usize {
        self.node.len()
    }

    /// No particles.
    pub fn is_empty(&self) -> bool {
        self.node.is_empty()
    }

    fn add(&mut self, c: &Chain, children: &[Vec<NodeIndex>], frame: Option<NodeIndex>) {
        // Preorder from the root, skipping ignored subtrees: (node, parent particle within the chain, depth).
        let mut order: Vec<(Option<NodeIndex>, u32, u32)> = Vec::new();
        let mut stack = vec![(c.root, NO_PARENT, 0)];
        while let Some((n, parent, depth)) = stack.pop() {
            if order.len() == MAX_PARTICLES {
                break;
            }
            let me = order.len() as u32;
            order.push((Some(n), parent, depth));
            for &k in children[n as usize].iter().rev() {
                if !c.ignore.contains(&k) {
                    stack.push((k, me, depth + 1));
                }
            }
        }
        let has_child = |i: usize, order: &[(Option<NodeIndex>, u32, u32)]| order.iter().any(|o| o.1 == i as u32);
        if c.endpoint != [0.0; 3] {
            for i in 0..order.len() {
                if !has_child(i, &order) && order.len() < MAX_PARTICLES {
                    order.push((None, i as u32, order[i].2 + 1));
                }
            }
        }
        let root_children = order.iter().filter(|o| o.1 == 0).count();
        let ignore_root = root_children > 1 && c.multi_child == MultiChild::Ignore;
        let deepest = order.iter().map(|o| o.2).max().unwrap_or(0).max(1) as f32;
        let first = self.node.len() as u32;
        let advanced = c.integration == Integration::Advanced;
        for &(node, parent, depth) in &order {
            let d = depth as f32 / deepest;
            let unit = |g: &Graded| eval(g, d).clamp(0.0, 1.0);
            self.node.push(node);
            self.parent
                .push(if parent == NO_PARENT { NO_PARENT } else { first + parent });
            self.pinned.push(parent == NO_PARENT || (ignore_root && parent == 0));
            self.endpoint.push(if node.is_none() { c.endpoint } else { [0.0; 3] });
            self.pull.push(unit(&c.pull));
            self.spring.push(unit(&c.spring));
            self.stiffness.push(if advanced { unit(&c.stiffness) } else { 0.0 });
            self.gravity.push(eval(&c.gravity, d).clamp(-1.0, 1.0));
            self.gravity_falloff.push(unit(&c.gravity_falloff));
            self.immobile.push(unit(&c.immobile));
            self.max_stretch.push(eval(&c.max_stretch, d).max(0.0));
            self.max_squish.push(unit(&c.max_squish));
        }
        self.chains.push(ChainRig {
            root: c.root,
            frame,
            first,
            count: order.len() as u32,
            advanced,
            immobile_world: c.immobile_type == ImmobileType::World,
            // A root with one child aims at it whatever the mode.
            multi_child: if root_children > 1 {
                c.multi_child
            } else {
                MultiChild::First
            },
            allow_grab: c.allow_grab,
            allow_posing: c.allow_posing,
        });
    }
}

/// A graded value at normalised depth `d`: its value times the curve, linear between points and held past the ends.
pub fn eval(g: &Graded, d: f32) -> f32 {
    let c = &g.curve;
    let factor = match c.iter().position(|p| p[0] >= d) {
        _ if c.is_empty() => 1.0,
        Some(0) => c[0][1],
        None => c[c.len() - 1][1],
        Some(i) => {
            let ([d0, f0], [d1, f1]) = (c[i - 1], c[i]);
            if d1 > d0 {
                f0 + (f1 - f0) * (d - d0) / (d1 - d0)
            } else {
                f1
            }
        }
    };
    g.value * factor
}
