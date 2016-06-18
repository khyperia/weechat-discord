extern { fn hello(); }

fn it_works() {
    unsafe { hello(); }
}
