fn main() {
    #[cfg(feature = "opengl3")]
    {
        use std::{env, fs::File, path::Path};

        use gl_generator::{Api, Fallbacks, Profile, Registry, StructGenerator};

        let dest = env::var("OUT_DIR").unwrap();
        let mut file = File::create(Path::new(&dest).join("gl_bindings.rs")).unwrap();

        Registry::new(Api::Gl, (3, 3), Profile::Core, Fallbacks::All, [])
            .write_bindings(StructGenerator, &mut file)
            .unwrap();
    }
}
