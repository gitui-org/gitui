use super::Scene;

/// A deterministic generator, so a generated topology is reproducible
/// from its seed alone without pulling in a property-testing
/// dependency.
struct Rng(u64);

impl Rng {
	const fn new(seed: u64) -> Self {
		// Keep the low bit of the state moving.
		Self(seed.wrapping_mul(2).wrapping_add(1))
	}

	fn next(&mut self) -> u64 {
		self.0 = self
			.0
			.wrapping_mul(6_364_136_223_846_793_005)
			.wrapping_add(1_442_695_040_888_963_407);
		self.0 ^ (self.0 >> 33)
	}

	/// A value in `0..upper`, or `0` when `upper` is zero.
	fn below(&mut self, upper: usize) -> usize {
		let upper = u64::try_from(upper).unwrap_or(u64::MAX).max(1);
		usize::try_from(self.next() % upper).unwrap_or(0)
	}

	/// True one time in `odds`.
	fn chance(&mut self, odds: usize) -> bool {
		self.below(odds) == 0
	}
}

/// The kind of history to generate.
#[derive(Clone, Copy, Debug)]
pub enum Shape {
	/// A single lane, each commit the parent of the one above it.
	Linear,
	/// Long-running branches that merge back often.
	MergeHeavy,
	/// Short topic branches off a trunk, merged or abandoned.
	Topics,
	/// Several disjoint histories, each ending in its own root.
	ManyRoots,
	/// Merges whose own first parent is a commit later merges reach
	/// too.
	///
	/// Both parents are drawn from the same spread, so a mainline can
	/// skip over a side branch the way a real one does. Shapes whose
	/// first parent is always the very next commit cannot produce
	/// that, and miss merges whose second-parent lanes are shared
	/// across several rows.
	Entangled,
	/// Parent lists Git permits and a walk has to survive: duplicated
	/// entries, octopus merges, and parents a clock-skewed walk
	/// reaches before their children.
	Adversarial,
}

impl Shape {
	pub const ALL: [Self; 6] = [
		Self::Linear,
		Self::MergeHeavy,
		Self::Topics,
		Self::ManyRoots,
		Self::Entangled,
		Self::Adversarial,
	];
}

fn push_later(
	parents: &mut Vec<usize>,
	name: usize,
	remaining: usize,
	offset: usize,
) {
	if offset > 0 && offset <= remaining {
		parents.push(name + offset);
	}
}

/// Generate a history of `commits` rows, newest first.
///
/// Commits are named `1..=commits` and listed in that order, so a
/// parent named above its child is one the walk reaches later: the
/// ordering a topological walk produces. [`Shape::Adversarial`] breaks
/// that on purpose for some parents, which is what a repository with
/// skewed commit times hands the walker.
pub fn generate(
	seed: u64,
	commits: usize,
	shape: Shape,
) -> Vec<(usize, Vec<usize>)> {
	let mut rng = Rng::new(seed);
	let mut history = Vec::with_capacity(commits);

	for index in 0..commits {
		let name = index + 1;
		let left = commits - index - 1;
		let mut parents: Vec<usize> = Vec::new();

		match shape {
			Shape::Linear => {
				push_later(&mut parents, name, left, 1);
			}
			Shape::MergeHeavy => {
				push_later(&mut parents, name, left, 1);
				if rng.chance(3) {
					let offset = 2 + rng.below(10);
					push_later(&mut parents, name, left, offset);
				}
			}
			Shape::Topics => {
				let offset = 1 + rng.below(3);
				push_later(&mut parents, name, left, offset);
				if rng.chance(4) {
					let offset = 2 + rng.below(20);
					push_later(&mut parents, name, left, offset);
				}
			}
			Shape::ManyRoots => {
				if !rng.chance(8) {
					push_later(&mut parents, name, left, 1);
				}
			}
			Shape::Entangled => {
				const SPAN: usize = 10;
				let offset = 1 + rng.below(SPAN);
				push_later(&mut parents, name, left, offset);
				if rng.chance(2) {
					let offset = 1 + rng.below(SPAN);
					push_later(&mut parents, name, left, offset);
				}
			}
			Shape::Adversarial => {
				push_later(&mut parents, name, left, 1);
				if rng.chance(3) {
					let offset = 1 + rng.below(6);
					push_later(&mut parents, name, left, offset);
				}
				if rng.chance(5) {
					let offset = 1 + rng.below(6);
					push_later(&mut parents, name, left, offset);
				}
				// A parent the walk has already passed, and a
				// repeated entry. Both are legal in a commit object
				// and neither may open a lane.
				if rng.chance(4) && index > 0 {
					parents.push(1 + rng.below(index));
				}
				if rng.chance(3) {
					if let Some(first) = parents.first().copied() {
						parents.push(first);
					}
				}
			}
		}

		history.push((name, parents));
	}

	history
}

/// Walk a generated history and hold it to every invariant.
pub fn walk_generated(
	seed: u64,
	commits: usize,
	shape: Shape,
) -> Scene {
	let history = generate(seed, commits, shape);
	let borrowed = history
		.iter()
		.map(|(commit, parents)| (*commit, parents.as_slice()))
		.collect::<Vec<_>>();

	Scene::walk(&borrowed)
}
