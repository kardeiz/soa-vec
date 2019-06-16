#![allow(non_snake_case)]
#![feature(allocator_api, alloc_layout_extra)]

use std::{
	slice::*,
	alloc::*,
	ptr::*,
	marker::*,
	cmp::*,
};
use second_stack::*;

/// This macro defines a struct-of-arrays style struct.
/// It need not be called often, just once per count of generic parameters.
macro_rules! soa {
	($name:ident, $L:ident, $t1:ident, $($ts:ident),+) => {

		/// Stores slices in a struct-of-arrays style
		/// with the API of Vec. The advantage over simply
		/// using multiple Vec is that all slices live in a single allocation,
		/// there's one shared len/capacity variable, and the API ensures
		/// that items are kept together through all operations like push/pop/sort
		pub struct $name<$t1: Sized $(, $ts: Sized)*> {
			len: usize,
			capacity: usize,
			$t1: NonNull<$t1>,
			$($ts: NonNull<$ts>,)*
			_marker: (PhantomData<$t1> $(, PhantomData<$ts>)*),
		}

		impl<$t1: Sized $(, $ts: Sized)*> $name<$t1 $(, $ts)*> {
			pub fn new() -> $name<$t1 $(, $ts)*> {
				$name {
					len: 0,
					capacity: 0,
					$t1: NonNull::dangling(),
					$($ts: NonNull::dangling(),)*
					_marker: (PhantomData $(, PhantomData::<$ts>)*),
				}
			}

			fn dealloc(&mut self) {
				if self.capacity > 0 {
					let layout = Self::layout_for_capacity(self.capacity).layout;
					unsafe { Global.dealloc(self.$t1.cast::<u8>(), layout) }
				}
			}

			/// Allocates and partitions a new region of uninitialized memory
			fn alloc(capacity: usize) -> (NonNull<$t1> $(, NonNull<$ts>)*) {
				unsafe {
					let layouts = Self::layout_for_capacity(capacity);
					let bytes = Global.alloc(layouts.layout).unwrap();
					(
						bytes.cast::<$t1>()
						$(, NonNull::new_unchecked(bytes.as_ptr().add(layouts.$ts) as *mut $ts))*
					)
				}
			}

			fn check_grow(&mut self) {
				unsafe {
					if self.len == self.capacity {
						let capacity = (self.capacity * 2).max(4);
						let ($t1 $(, $ts)*) = Self::alloc(capacity);

						copy_nonoverlapping(self.$t1.as_ptr(), $t1.as_ptr(), self.len);
						$(
							copy_nonoverlapping(self.$ts.as_ptr(), $ts.as_ptr(), self.len);
						)*

						self.dealloc();

						// Assign
						self.$t1 = $t1;
						$(self.$ts = $ts;)*
						self.capacity = capacity;

					}
				}
			}

			#[inline(always)]
			pub fn len(&self) -> usize { self.len }

			pub fn clear(&mut self) {
				while self.len > 0 {
					self.pop();
				}
			}

			pub fn push(&mut self, value: ($t1 $(, $ts)*)) {
				unsafe {
					self.check_grow();
					let ($t1 $(, $ts)*) = value;
					write(self.$t1.as_ptr().add(self.len), $t1);
					$(write(self.$ts.as_ptr().add(self.len), $ts);)*
					self.len += 1;
				}
			}

			pub fn pop(&mut self) -> Option<($t1 $(, $ts)*)> {
				if self.len == 0 {
					None
				} else {
					self.len -= 1;
					unsafe {
						Some((
							read(self.$t1.as_ptr().add(self.len))
							$(, read(self.$ts.as_ptr().add(self.len)))*
						))
					}
				}
			}

			/// ##Panics:
			///  * Must panic if index is out of bounds
			pub fn swap_remove(&mut self, index: usize) -> ($t1 $(, $ts)*) {
				if index >= self.len {
					panic!("Index out of bounds");
				}

				unsafe {
					let $t1 = self.$t1.as_ptr().add(index);
					$(let $ts = self.$ts.as_ptr().add(index);)*

					let v = (
						read($t1)
						$(, read($ts))*
					);

					self.len -= 1;

					if self.len != index {
						copy_nonoverlapping(self.$t1.as_ptr().add(self.len), $t1, 1);
						$(copy_nonoverlapping(self.$ts.as_ptr().add(self.len), $ts, 1);)*
					}

					v
				}
			}

			fn layout_for_capacity(capacity: usize) -> $L {
				let layout = Layout::array::<$t1>(capacity).unwrap();

				$(let (layout, $ts) = layout.extend(Layout::array::<$ts>(capacity).unwrap()).unwrap();)*

				$L {
					layout
					$(, $ts)*
				}
			}

			#[inline(always)] // Inline for dead code elimination
			pub fn slices<'a>(&self) -> (&'a [$t1] $(, &'a [$ts])*) {
				unsafe {
					(
						from_raw_parts::<'a>(self.$t1.as_ptr(), self.len),
						$(from_raw_parts::<'a>(self.$ts.as_ptr(), self.len),)*
					)
				}
			}

			#[inline(always)] // Inline for dead code elimination
			pub fn iters<'a>(&self) -> (Iter<'a, $t1> $(, Iter<'a, $ts>)*) {
				unsafe {
					(
						from_raw_parts::<'a>(self.$t1.as_ptr(), self.len).iter()
						$(, from_raw_parts::<'a>(self.$ts.as_ptr(), self.len).iter())*
					)
				}
			}

			#[inline(always)] // Inline for dead code elimination
			pub fn slices_mut<'a>(&self) -> (&'a mut [$t1] $(, &'a mut [$ts])*) {
				unsafe {
					(
						from_raw_parts_mut::<'a>(self.$t1.as_ptr(), self.len),
						$(from_raw_parts_mut::<'a>(self.$ts.as_ptr(), self.len),)*
					)
				}
			}

			/// ## Panics
			/// * If index is >= len
			pub fn get<'a>(&self, index: usize) -> (&'a $t1 $(, &'a $ts)*) {
				unsafe {
					if index >= self.len {
						panic!("Index out of range");
					}

					(
						&*self.$t1.as_ptr().add(index)
						$(, &*self.$ts.as_ptr().add(index))*
					)
				}
			}

			pub fn sort_unstable_by<F: FnMut((&$t1 $(, &$ts)*), (&$t1 $(, &$ts)*))->Ordering>(&mut self, mut f: F) {
				if self.len < 2 {
					return;
				}
				let mut indices = acquire(0..self.len);

				indices.sort_unstable_by(|a, b| unsafe {
					f(
						(&*self.$t1.as_ptr().add(*a) $(, &*self.$ts.as_ptr().add(*a))*, ),
						(&*self.$t1.as_ptr().add(*b) $(, &*self.$ts.as_ptr().add(*b))*, ),
					)});

				// Example
				// c b d e a
				// 4 1 0 2 3 // indices
				// 2 1 3 4 0 // lookup

				let mut lookup = unsafe { acquire_uninitialized(self.len) };
				for (i, index) in indices.iter().enumerate() {
					lookup[*index] = i;
				}

				let ($t1 $(, $ts)*) = self.slices_mut();

				for i in 0..indices.len() {
					let dest = indices[i]; // The index that should go here
					if i != dest {
						// Swap
						$t1.swap(i, dest);
						$($ts.swap(i, dest);)*

						// Account for swaps that already happened
						indices[lookup[i]] = dest;
						lookup[dest] = lookup[i];
					}
				}
			}
		}

		struct $L {
			layout: Layout,
			$($ts: usize,)*
		}


		impl<$t1: Sized $(, $ts: Sized)*> Drop for $name<$t1 $(, $ts)*> {
			fn drop(&mut self) {
				self.clear(); // Drop owned items
				self.dealloc()
			}
		}


		impl<$t1: Clone + Sized $(, $ts: Clone + Sized)*> Clone for $name<$t1 $(, $ts)*> {
			fn clone(&self) -> Self {
				let capacity = self.len;
				if capacity == 0 {
					Self::new()
				} else {
					let ($t1 $(,$ts)*) = Self::alloc(capacity);

					unsafe {
						for i in 0..self.len {
							write($t1.as_ptr().add(i), (&*(self.$t1.as_ptr().add(i))).clone());
						}
						$(
							for i in 0..self.len {
								write($ts.as_ptr().add(i), (&*(self.$ts.as_ptr().add(i))).clone());
							}
						)*
					}

					Self {
						capacity,
						len: self.len,
						$t1: $t1,
						$($ts: $ts,)*
						_marker: (PhantomData $(, PhantomData::<$ts>)*),
					}
				}

			}
		}
	};
}

soa!(Soa2, _2, T1, T2);
soa!(Soa3, _3, T1, T2, T3);
soa!(Soa4, _4, T1, T2, T3, T4);



#[cfg(test)]
mod tests {
	use super::*;
	use testdrop::TestDrop;

	#[test]
	fn layouts_do_not_overlap() {
		// Trying with both (small, large) and (large, small) to ensure nothing bleeds into anything else.
		// This verifies we correctly chunk the slices from the larger allocations.
		let mut soa_ab = Soa2::new();
		let mut soa_ba = Soa2::new();

		fn ab(v: usize) -> (u8, f64) {
			(v as u8, 200.0 + ((v as f64) / 200.0))
		}

		fn ba(v: usize) -> (f64, u8) {
			(15.0 + ((v as f64) / 16.0), (200 - v) as u8)
		}

		// Combined with the tests inside, also verifies that we are copying the data on grow correctly.
		for i in 0..100 {
			soa_ab.push(ab(i));
			let (a, b) = soa_ab.slices();
			assert_eq!(i+1, a.len());
			assert_eq!(i+1, b.len());
			assert_eq!(ab(0).0, a[0]);
			assert_eq!(ab(0).1, b[0]);
			assert_eq!(ab(i).0, a[i]);
			assert_eq!(ab(i).1, b[i]);

			soa_ba.push(ba(i));
			let (b, a) = soa_ba.slices();
			assert_eq!(i+1, a.len());
			assert_eq!(i+1, b.len());
			assert_eq!(ba(0).0, b[0]);
			assert_eq!(ba(0).1, a[0]);
			assert_eq!(ba(i).0, b[i]);
			assert_eq!(ba(i).1, a[i]);
		}
	}

	#[test]
	fn sort() {
		let mut soa = Soa3::new();

		soa.push((3, 'a', 4.0));
		soa.push((1, 'b', 5.0));
		soa.push((2, 'c', 6.0));

		soa.sort_unstable_by(|(a1, _, _), (a2, _, _)| a1.cmp(a2));

		assert_eq!(soa.get(0), (&1, &('b'), &5.0));
		assert_eq!(soa.get(1), (&2, &('c'), &6.0));
		assert_eq!(soa.get(2), (&3, &('a'), &4.0));
	}

	#[test]
	fn drops() {
		let td = TestDrop::new();
		let (id, item) = td.new_item();
		{
			let mut soa = Soa2::new();
			soa.push((1.0, item));

			// Did not drop when moved into the vec
			td.assert_no_drop(id);

			// Did not drop through resizing the vec.
			for _ in 0..50 {
				soa.push((2.0, td.new_item().1));
			}
			td.assert_no_drop(id);
		}
		// Dropped with the vec
		td.assert_drop(id);
	}

	#[test]
	fn clones() {
		let mut src = Soa2::new();
		src.push((1.0, 2.0));
		src.push((3.0, 4.0));

		let dst = src.clone();
		assert_eq!(dst.len(), 2);
		assert_eq!(dst.get(0), (&1.0, &2.0));
		assert_eq!(dst.get(1), (&3.0, &4.0));
	}
}