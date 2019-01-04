use gfx2_extension_runtime::{load_dev_dylib, load_module};
use std::env;
use std::thread::sleep;
use std::time::Duration;
use test_dylib;

#[test]
fn test_reload() {
    env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).unwrap();

    for _ in 0..10 {
        let lib = load_dev_dylib!(test_dylib).unwrap();
        let hot = load_module!(&lib, test_dylib::hot).unwrap();

        let mut test_vec = Vec::new();
        hot.push(&mut test_vec);
        assert_eq!(&test_vec[..], &[&42]);
        //sleep(Duration::from_secs());
        eprintln!("reloading...");
    }
}

pub mod hot {
    #[doc(hidden)]
    pub mod __load {
        pub struct DllShims<'__lib> {
            fnptr_shorten_lifetime: ::libloading::Symbol<'__lib, *const ::std::ffi::c_void>,
            fnptr_push: ::libloading::Symbol<'__lib, *const ::std::ffi::c_void>,
            fnptr_simple: ::libloading::Symbol<'__lib, *const ::std::ffi::c_void>,
            pub STRING: &'__lib &'__lib str,
        }
        impl<'__lib> DllShims<'__lib> {
            pub fn shorten_lifetime<'a, 'b, 'min>(&self, arg0: &'a i32, arg1: &'b i32) -> &'min i32
            where
                'a: 'min,
                'b: 'min,
                '__lib: 'a + 'b + 'min,
            {
                (unsafe {
                    ::std::mem::transmute::<_, fn(a: &'a i32, b: &'b i32) -> &'min i32>(
                        *self.fnptr_shorten_lifetime,
                    )
                })(arg0, arg1)
            }
            pub fn push<'a, 'b>(&self, arg0: &'a mut Vec<&'b i32>)
            where
                '__lib: 'a + 'b,
            {
                (unsafe {
                    ::std::mem::transmute::<_, fn(v: &'a mut Vec<&'b i32>)>(*self.fnptr_push)
                })(arg0)
            }
            pub fn simple(&self, arg0: i32) -> i32 {
                (unsafe { ::std::mem::transmute::<_, fn(a: i32) -> i32>(*self.fnptr_simple) })(arg0)
            }
            pub fn load(lib: &'__lib ::libloading::Library) -> ::libloading::Result<Self> {
                Ok(Self {
                    fnptr_shorten_lifetime: unsafe {
                        lib.get(stringify!(shorten_lifetime).as_bytes())?
                    },
                    fnptr_push: unsafe { lib.get(stringify!(push).as_bytes())? },
                    fnptr_simple: unsafe { lib.get(stringify!(simple).as_bytes())? },
                })
            }
        }
    }
    #[no_mangle]
    pub extern "C" fn shorten_lifetime<'a, 'b, 'min>(a: &'a i32, b: &'b i32) -> &'min i32
    where
        'a: 'min,
        'b: 'min,
    {
        if *a > *b {
            a
        } else {
            b
        }
    }
    #[no_mangle]
    pub extern "C" fn push<'a, 'b>(v: &'a mut Vec<&'b i32>) {
        v.push(&42);
    }
    #[no_mangle]
    pub extern "C" fn simple(a: i32) -> i32 {
        eprintln!("you called? {}", a);
        a + 1
    }
    #[no_mangle]
    pub const STRING: &str = "Hello!";
}
