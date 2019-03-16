extern crate nonblockinghashmap;
extern crate rand;
use	nonblockinghashmap::{ConcurrentMap, print_all};
use std::thread::spawn;
use std::sync::Arc;

fn main(){
	let newmap = ConcurrentMap::new_with_size(1000);
	let shared_map = Arc::new(newmap);
	let nthreads = 30;
	let put = 1000;
	let get = 100000;

	let threads: Vec<_> = (0..nthreads).flat_map(|n| {
		let child_map_put = shared_map.clone();
		let child_map_get = shared_map.clone();
		let writer = spawn(move|| {
			for i in 0..put {
				child_map_put.as_mut().put(format!("key {}", i), format!("value {} t {}", i, n));
			}
		});

		let reader = spawn(move|| {
			for i in 0..get {
				let key = format!("key {}", i % put);
				child_map_get.as_mut().get(key);
			}
		} );
		vec![writer, reader]
	}).collect();
	for t in threads {
		t.join().expect("Error joining");
	}
	print_all(&Arc::try_unwrap(shared_map).unwrap().as_mut());
}
