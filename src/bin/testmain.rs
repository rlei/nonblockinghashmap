extern crate nonblockinghashmap;
extern crate rand;
use	nonblockinghashmap::{NonBlockingHashMap, print_all};
use std::thread::spawn;
use std::sync::Arc;
use std::cell::UnsafeCell;

#[derive(Debug)]
struct SharedMap<K, V>(UnsafeCell<NonBlockingHashMap<K, V>>);

unsafe impl<K, V> Sync for SharedMap<K, V> {}

fn main(){
	let newmap = NonBlockingHashMap::<String,String>::new_with_size(1000);
	let unsafe_newmap = SharedMap(UnsafeCell::new(newmap));
	let shared_map = Arc::new(unsafe_newmap);
	let nthreads = 30;
	let put = 1000;
	let get = 100000;

	// let (noti_chan, noti_recv) = mpsc::channel();
	let threads: Vec<_> = (0..nthreads).flat_map(|n| {
		let child_map_put = shared_map.clone();
		let child_map_get = shared_map.clone();
		// let noti_chan_clone_put = noti_chan.clone();
		// let noti_chan_clone_get = noti_chan.clone();
		let writer = spawn(move|| {
			for i in 0..put {
				unsafe{(*child_map_put.0.get()).put(format!("key {}", i), format!("value {} t {}", i, n))};
			}
			// noti_chan_clone_put.send(()).expect("send channel error");
		});

		let reader = spawn(move|| {
			for i in 0..get {
				let key = format!("key {}", i % put);
				unsafe{(*child_map_get.0.get()).get(key)};
				//println!("(key, value) = ({}, {})", key.clone(), (*child_map_get.get()).get(key));
			}
			// noti_chan_clone_get.send(()).expect("send channel error");
		} );
		vec![writer, reader]
	}).collect();
	for t in threads {
		t.join().expect("Error joining");
	}
	// for _ in 0..nthreads*2 {
	// 	noti_recv.recv().expect("recv channel error");	
	// }
	print_all(&Arc::try_unwrap(shared_map).unwrap().0.into_inner());
}
