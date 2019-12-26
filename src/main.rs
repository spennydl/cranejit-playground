use cranelift::prelude::*;

use cranelift::prelude::*;
use cranelift_module::{DataContext, Linkage, Module};
use cranelift_simplejit::{SimpleJITBackend, SimpleJITBuilder};
use std::process;

/// The basic JIT class.
pub struct JIT {
    /// The function builder context, which is reused across multiple
    /// FunctionBuilder instances.
    builder_context: FunctionBuilderContext,

    /// The main Cranelift context, which holds the state for codegen. Cranelift
    /// separates this from `Module` to allow for parallel compilation, with a
    /// context per thread, though this isn't in the simple demo here.
    ctx: codegen::Context,

    /// The data context, which is to data objects what `ctx` is to functions.
    data_ctx: DataContext,

    /// The module, with the simplejit backend, which manages the JIT'd
    /// functions.
    module: Module<SimpleJITBackend>,
}

impl JIT {
    /// Create a new `JIT` instance.
    pub fn new() -> Self {
        // Windows calling conventions are not supported yet.
        if cfg!(windows) {
            unimplemented!();
        }

        let builder = SimpleJITBuilder::new(cranelift_module::default_libcall_names());
        let module = Module::new(builder);
        Self {
            builder_context: FunctionBuilderContext::new(),
            ctx: module.make_context(),
            data_ctx: DataContext::new(),
            module,
        }
    }

    pub fn make_prog(&mut self, name: &str) -> Result<*const u8, String> {
        // build our function definition
        self.build_function()?;

        // declare a func to simplejit
        let id = self.module
            .declare_function(name, Linkage::Export, &self.ctx.func.signature)
            .map_err(|e| e.to_string())?;

        // define the function
        self.module
            .define_function(id, &mut self.ctx)
            .map_err(|e| e.to_string())?;

        // compilation is done, finalize and clean up
        self.module.clear_context(&mut self.ctx);
        self.module.finalize_definitions();

        let code = self.module.get_finalized_function(id);

        Ok(code)
    }

    fn build_function(&mut self) -> Result<(), String> {
        // we will have 2 params and add them together (for now)
        let int_type = self.module.target_config().pointer_type();
        self.ctx.func.signature.params.push(AbiParam::new(int_type));
        self.ctx.func.signature.params.push(AbiParam::new(int_type));

        // TODO: do we need a return?
        self.ctx.func.signature.returns.push(AbiParam::new(int_type));

        let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_context);

        // declare the EBB
        let ebb = builder.create_ebb();
        // do this before we declare our variables
        builder.append_ebb_params_for_function_params(ebb);
        builder.switch_to_block(ebb);
        builder.seal_block(ebb); // this is the top-level ebb so we can seal it here

        // need an idx for vars?
        let mut var_idx: usize = 0;

        // the vars we will use
        let arg1_val = builder.ebb_params(ebb)[0];
        let arg2_val = builder.ebb_params(ebb)[1];

        let arg1 = Variable::new(var_idx); var_idx += 1;
        let arg2 = Variable::new(var_idx); var_idx += 1;
        let retval = Variable::new(var_idx); var_idx += 1;

        // declare, then define variables
        builder.declare_var(arg1, int_type);
        builder.def_var(arg1, arg1_val);

        builder.declare_var(arg2, int_type);
        builder.def_var(arg2, arg2_val);

        builder.declare_var(retval, int_type);
        let zero = builder.ins().iconst(int_type, 0);
        builder.def_var(retval, zero); // TODO: do we need to do this before using it?  i assume so...

        // we are set up!  emit code to add the args together
        // and write to the return val
        let lhs = builder.use_var(arg1);
        let rhs = builder.use_var(arg2);
        let val = builder.ins().iadd(lhs, rhs);
        builder.def_var(retval, val);

        // do the return
        let ret = builder.use_var(retval);
        builder.ins().return_(&[ret]); // TODO: takes a slice?  does it support multiple returns?
        builder.finalize(); // call it good

        Ok(())
    }
}

fn main() {
    let mut jit = JIT::new();

    // build the code, get a ptr to it
    let code = jit.make_prog("derpadd".into()).unwrap_or_else(|e| {
        println!("whoops: {}", e);
        process::exit(1);
    });

    // transmute the ptr to a function
    let f = unsafe { std::mem::transmute::<_, fn(isize, isize) -> isize>(code) };

    let result = f(10, 4);
    println!("10 + 4 = {}", result);

    let result = f(24, 7);
    println!("24 + 7 = {}", result);
}
