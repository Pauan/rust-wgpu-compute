use std::borrow::Cow;
use quote::{quote, format_ident};
use naga::{Arena, Handle, Span};
use naga::ir::*;

use codespan_reporting::diagnostic::{Diagnostic, Label, LabelStyle};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;


pub struct GenerateError {
    pub message: String,
}


#[derive(Debug)]
struct Constant {
    name: String,
    ty: RustType,
    value: proc_macro2::TokenStream,
}


#[derive(Debug)]
struct Variable {
    name: String,
    space: AddressSpace,
    binding: u32,
    ty: RustType,
}


#[derive(Debug)]
enum RustType {
    Single {
        tokens: proc_macro2::TokenStream,
    },
    Vec {
        inner: proc_macro2::TokenStream,
    },
    Array {
        inner: proc_macro2::TokenStream,
        size: u32,
    },
}

impl RustType {
    fn tokens(&self) -> Cow<'_, proc_macro2::TokenStream> {
        match self {
            Self::Single { tokens } => Cow::Borrowed(tokens),
            Self::Vec { inner } => Cow::Owned(quote! { ::std::vec::Vec<#inner> }),
            Self::Array { inner, size } => Cow::Owned(quote! { [#inner; #size] }),
        }
    }

    /// Type of a single element
    fn single(&self) -> &proc_macro2::TokenStream {
        match self {
            Self::Single { tokens } => tokens,
            Self::Vec { inner } => inner,
            Self::Array { inner, size: _ } => inner,
        }
    }

    /// Size of a single element
    fn single_size_of(&self) -> proc_macro2::TokenStream {
        let single = self.single();

        quote! { ::std::mem::size_of::<#single>() }
    }
}


pub struct Generate {
    pub source: String,
    pub path: String,
    pub module: Module,
}

impl Generate {
    fn error(&self, span: Span, message: &str) -> GenerateError {
        let config = term::Config::default();

        let files = SimpleFile::new(self.path.clone(), self.source.clone());

        let diagnostic = Diagnostic::error()
            .with_message(message)
            .with_label(Label {
                style: LabelStyle::Primary,
                file_id: (),
                range: span.to_range().unwrap(),
                message: message.to_string(),
            });

        let message = term::emit_into_string(&config, &files, &diagnostic).unwrap();

        GenerateError {
            message,
        }
    }


    fn vector_size(size: VectorSize) -> usize {
        match size {
            VectorSize::Bi => 2,
            VectorSize::Tri => 3,
            VectorSize::Quad => 4,
        }
    }


    fn scalar_to_rust(&self, span: Span, ty: Scalar) -> Result<proc_macro2::TokenStream, GenerateError> {
        match ty.kind {
            ScalarKind::Sint => {
                match ty.width {
                    1 => Ok(quote! { i8 }),
                    2 => Ok(quote! { i16 }),
                    4 => Ok(quote! { i32 }),
                    8 => Ok(quote! { i64 }),
                    16 => Ok(quote! { i128 }),
                    _ => Err(self.error(span, "Integers cannot be larger than i128")),
                }
            },
            ScalarKind::Uint => {
                match ty.width {
                    1 => Ok(quote! { u8 }),
                    2 => Ok(quote! { u16 }),
                    4 => Ok(quote! { u32 }),
                    8 => Ok(quote! { u64 }),
                    16 => Ok(quote! { u128 }),
                    _ => Err(self.error(span, "Integers cannot be larger than u128")),
                }
            },
            ScalarKind::Float => {
                match ty.width {
                    2 => Ok(quote! { f16 }),
                    4 => Ok(quote! { f32 }),
                    8 => Ok(quote! { f64 }),
                    _ => Err(self.error(span, "Floats must be f16, f32, or f64")),
                }
            },
            ScalarKind::Bool => {
                Ok(quote! { bool })
            },
            _ => {
                Err(self.error(span, "Unknown scalar type"))
            },
        }
    }


    fn parse_type(&self, handle: Handle<Type>) -> Result<RustType, GenerateError> {
        let ty = &self.module.types[handle];

        let span = self.module.types.get_span(handle);

        if let Some(name) = &ty.name {
            let name = format_ident!("{}", name);

            Ok(RustType::Single {
                tokens: quote! { #name },
            })

        } else {
            match ty.inner {
                TypeInner::Array { base, size, stride: _ } => {
                    let inner = self.parse_type(base)?;
                    let inner = inner.tokens().into_owned();

                    match size {
                        ArraySize::Dynamic => {
                            Ok(RustType::Vec {
                                inner,
                            })
                        },
                        ArraySize::Constant(size) => {
                            let size = size.get();

                            Ok(RustType::Array {
                                inner,
                                size,
                            })
                        },
                        ArraySize::Pending(_) => todo!(),
                    }
                },
                TypeInner::Scalar(scalar) => {
                    Ok(RustType::Single {
                        tokens: self.scalar_to_rust(span, scalar)?,
                    })
                },
                TypeInner::Vector { size, scalar } => {
                    let size = Self::vector_size(size);

                    let scalar = self.scalar_to_rust(span, scalar)?;

                    Ok(RustType::Single {
                        tokens: quote! { [#scalar; #size] },
                    })
                },
                TypeInner::Matrix { columns, rows, scalar } => {
                    let columns = Self::vector_size(columns);
                    let rows = Self::vector_size(rows);

                    let scalar = self.scalar_to_rust(span, scalar)?;

                    Ok(RustType::Single {
                        tokens: quote! { [[#scalar; #rows]; #columns] },
                    })
                },
                _ => {
                    Err(self.error(span, "Unknown type"))
                },
            }
        }
    }


    fn parse_typedef(&self, handle: Handle<Type>) -> Result<proc_macro2::TokenStream, GenerateError> {
        let ty = &self.module.types[handle];

        let name = format_ident!("{}", ty.name.as_ref().unwrap());

        match &ty.inner {
            // TODO handle span
            TypeInner::Struct { members, span: _ } => {
                let mut fields = vec![];
                let mut types = vec![];

                // TODO handle offset
                for member in members.iter() {
                    let name = member.name.as_ref().ok_or_else(|| self.error(self.module.types.get_span(handle), "Missing name on struct field"))?;
                    fields.push(format_ident!("{}", name));

                    let ty = self.parse_type(member.ty)?;
                    types.push(ty.tokens().into_owned());
                }

                Ok(quote! {
                    #[derive(
                        ::std::fmt::Debug,
                        ::std::clone::Clone,
                        ::std::marker::Copy,
                        ::bytemuck::NoUninit,
                        ::bytemuck::AnyBitPattern,
                    )]
                    #[repr(C)]
                    pub struct #name {
                        #(pub #fields: #types,)*
                    }

                    impl #name {
                        pub fn new(#(#fields: #types),*) -> Self {
                            Self {
                                #(#fields),*
                            }
                        }
                    }
                })
            },
            _ => {
                let ty = self.parse_type(handle)?;
                let ty = ty.tokens();

                Ok(quote! {
                    pub type #name = #ty;
                })
            },
        }
    }


    fn parse_compose(&self, span: Span, handle: Handle<Type>, values: Vec<proc_macro2::TokenStream>) -> Result<proc_macro2::TokenStream, GenerateError> {
        let ty = &self.module.types[handle];

        match &ty.inner {
            TypeInner::Vector { .. } => {
                Ok(quote! { [#(#values),*] })
            },

            TypeInner::Matrix { .. } => {
                Ok(quote! { [#(#values),*] })
            },

            TypeInner::Struct { members, span: _ } => {
                let name = format_ident!("{}", ty.name.as_ref().unwrap());

                let fields = members.into_iter()
                    .map(|member| format_ident!("{}", member.name.as_ref().unwrap()))
                    .collect::<Vec<_>>();

                Ok(quote! {
                    #name {
                        #(#fields: #values,)*
                    }
                })
            },

            _ => Err(self.error(span, "Unknown type constructor")),
        }
    }


    fn parse_expression(&self, arena: &Arena<Expression>, handle: Handle<Expression>) -> Result<proc_macro2::TokenStream, GenerateError> {
        let expression = &arena[handle];

        match expression {
            Expression::Literal(literal) => Ok(match literal {
                Literal::F64(x) => quote! { #x },
                Literal::F32(x) => quote! { #x },
                Literal::F16(x) => {
                    let x = x.to_f32();
                    quote! { #x }
                },
                Literal::U32(x) => quote! { #x },
                Literal::I32(x) => quote! { #x },
                Literal::U64(x) => quote! { #x },
                Literal::I64(x) => quote! { #x },
                Literal::Bool(x) => quote! { #x },
                Literal::AbstractInt(x) => quote! { #x },
                Literal::AbstractFloat(x) => quote! { #x },
            }),

            Expression::Constant(handle) => {
                let value = &self.module.constants[*handle];

                if let Some(name) = &value.name {
                    let name = format_ident!("{}", name);
                    Ok(quote! { #name })

                } else {
                    self.parse_expression(&self.module.global_expressions, value.init)
                }
            },

            Expression::Override(handle) => {
                let value = &self.module.overrides[*handle];

                if let Some(name) = &value.name {
                    let name = format_ident!("{}", name);
                    Ok(quote! { #name })

                } else if let Some(init) = value.init {
                    self.parse_expression(&self.module.global_expressions, init)

                } else {
                    Err(self.error(self.module.overrides.get_span(*handle), "override must have a default value"))
                }
            },

            Expression::ZeroValue(handle) => todo!(),

            Expression::Compose { ty, components } => {
                let values = components.into_iter().map(|value| {
                    self.parse_expression(arena, *value)
                }).collect::<Result<Vec<_>, _>>()?;

                self.parse_compose(arena.get_span(handle), *ty, values)
            },

            _ => todo!(),
        }
    }


    pub fn to_tokens(&self, source: String) -> Result<proc_macro2::TokenStream, GenerateError> {
        let mut typedefs: Vec<proc_macro2::TokenStream> = vec![];

        let mut constants: Vec<Constant> = vec![];

        let mut variables: Vec<Vec<Variable>> = vec![];

        for (handle, ty) in self.module.types.iter() {
            if ty.name.is_some() {
                typedefs.push(self.parse_typedef(handle)?);
            }
        }

        for (_, constant) in self.module.constants.iter() {
            if let Some(name) = &constant.name {
                constants.push(Constant {
                    name: name.to_string(),
                    ty: self.parse_type(constant.ty)?,
                    value: self.parse_expression(&self.module.global_expressions, constant.init)?,
                });
            }
        }

        for (_, variable) in self.module.global_variables.iter() {
            if let Some(ResourceBinding { group, binding }) = variable.binding {
                if let Some(name) = &variable.name {
                    let index = group as usize;

                    while variables.len() <= index {
                        variables.push(vec![]);
                    }

                    variables[index].push(Variable {
                        name: name.to_string(),
                        space: variable.space,
                        binding,
                        ty: self.parse_type(variable.ty)?,
                    });
                }
            }
        }


        let constants = constants.into_iter().map(|constant| {
            let name = format_ident!("{}", constant.name);
            let ty = constant.ty.tokens();
            let value = constant.value;
            quote! { pub const #name: #ty = #value; }
        }).collect::<Vec<_>>();


        let bindings = variables.iter().flatten().map(|input| {
            let name = format_ident!("{}", input.name);
            let ty = input.ty.tokens();
            quote! { pub #name: #ty, }
        }).collect::<Vec<_>>();

        let buffers = variables.iter().flatten().map(|input| {
            let name = format_ident!("{}", input.name);
            let ty = input.ty.tokens();
            quote! { pub #name: ::wgpu_compute::Input<#ty>, }
        }).collect::<Vec<_>>();

        let to_buffers = variables.iter().flatten().map(|input| {
            let name = format_ident!("{}", input.name);

            match input.ty {
                RustType::Vec { .. } => {
                    quote! { #name: gpu.input_vec(self.#name.as_slice()), }
                },
                _ => {
                    quote! { #name: gpu.input(&self.#name), }
                },
            }
        }).collect::<Vec<_>>();


        let bind_group_layouts = variables.iter().map(|group| {
            let group = group.into_iter().map(|input| {
                let size_of = input.ty.single_size_of();
                let binding = input.binding;

                let space = match input.space {
                    AddressSpace::Uniform => {
                        quote! { ::wgpu::BufferBindingType::Uniform }
                    },
                    AddressSpace::Storage { access } => {
                        let read_only = !access.contains(StorageAccess::STORE);

                        quote! { ::wgpu::BufferBindingType::Storage { read_only: #read_only } }
                    },
                    _ => {
                        todo!();
                    },
                };

                quote! {
                    ::wgpu::BindGroupLayoutEntry {
                        binding: #binding,
                        visibility: ::wgpu::ShaderStages::COMPUTE,
                        ty: ::wgpu::BindingType::Buffer {
                            ty: #space,
                            has_dynamic_offset: false,
                            min_binding_size: ::std::option::Option::Some(
                                ::std::num::NonZeroU64::new(#size_of as u64).unwrap()
                            ),
                        },
                        count: ::std::option::Option::None,
                    },
                }
            }).collect::<Vec<_>>();

            quote! {
                &[#(#group)*]
            }
        }).collect::<Vec<_>>();


        let bind_groups = variables.iter().map(|group| {
            let group = group.into_iter().map(|input| {
                let name = format_ident!("{}", input.name);
                let binding = input.binding;

                quote! {
                    ::wgpu::BindGroupEntry {
                        binding: #binding,
                        resource: ::wgpu_compute::Gpu::bind_group(&self.#name),
                    },
                }
            }).collect::<Vec<_>>();

            quote! {
                &[#(#group)*]
            }
        }).collect::<Vec<_>>();


        let bindings = quote! {
            pub struct Bindings {
                #(#bindings)*
            }

            pub struct Buffers {
                #(#buffers)*
            }

            impl Buffers {
                ::std::thread_local! {
                    static GPU_LAYOUT: ::std::cell::OnceCell<::wgpu_compute::__internal::GpuLayout> =
                        ::std::cell::OnceCell::new();
                }

                fn gpu_layout<'a>(
                    gpu: &::wgpu_compute::Gpu,
                    layout: &'a ::std::cell::OnceCell<::wgpu_compute::__internal::GpuLayout>,
                ) -> &'a ::wgpu_compute::__internal::GpuLayout {
                    layout.get_or_init(|| {
                        ::wgpu_compute::__internal::GpuLayout::new(
                            gpu,
                            #source,
                            &[
                                #(#bind_group_layouts)*
                            ],
                        )
                    })
                }

                fn bind_group<A, F>(&self, f: F) -> A where F: FnOnce(&[&[::wgpu::BindGroupEntry]]) -> A {
                    f(&[
                        #(#bind_groups)*
                    ])
                }
            }

            impl ::wgpu_compute::ToBuffers for Bindings {
                type Output = Buffers;

                fn to_buffers(&self, gpu: &::wgpu_compute::Gpu) -> Self::Output {
                    Buffers {
                        #(#to_buffers)*
                    }
                }
            }
        };

        let functions = self.module.entry_points.iter().map(|entry| {
            if let ShaderStage::Compute = entry.stage {
                let entry_name = &entry.name;

                let name = format_ident!("{}", entry.name);
                let gpu_name = format_ident!("{}_gpu", entry.name);
                let cpu_name = format_ident!("{}_cpu", entry.name);

                let [x, y, z] = entry.workgroup_size;

                Ok(quote! {
                    pub async fn #name(state: ::wgpu_compute::State<Bindings, Buffers>, threads: usize) {
                        fn #gpu_name(
                            state: ::wgpu_compute::__internal::StateGpu<Buffers>,
                            threads: usize,
                        ) -> impl ::std::future::Future<Output = ()> + use<> {
                            ::std::thread_local! {
                                static GPU_FN: ::std::cell::OnceCell<::wgpu_compute::__internal::GpuFn> =
                                    ::std::cell::OnceCell::new();
                            }

                            Buffers::GPU_LAYOUT.with(|gpu_layout| {
                                let gpu_layout = Buffers::gpu_layout(&state.gpu, gpu_layout);

                                GPU_FN.with(|gpu_fn| {
                                    let gpu_fn = gpu_fn.get_or_init(|| {
                                        ::wgpu_compute::__internal::GpuFn::new(
                                            &state.gpu,
                                            gpu_layout,
                                            [#x, #y, #z],
                                            #entry_name,
                                        )
                                    });

                                    let mut encoder = state.buffers.bind_group(|bind_group| {
                                        ::wgpu_compute::__internal::command_encoder(
                                            &state.gpu,
                                            gpu_layout,
                                            gpu_fn,
                                            threads,
                                            bind_group,
                                        )
                                    });

                                    for (input, output) in state.copy_buffers {
                                        ::wgpu_compute::__internal::copy_input_to_output(&mut encoder, &input, &output);
                                    }

                                    ::wgpu_compute::__internal::wait(&state.gpu, encoder)
                                })
                            })
                        }

                        fn #cpu_name(
                            state: ::wgpu_compute::__internal::StateCpu<Bindings>,
                            threads: usize,
                        ) {
                            ::std::todo!()
                        }

                        let state: ::wgpu_compute::__internal::State<Bindings, Buffers> = ::std::convert::Into::into(state);

                        match state {
                            ::wgpu_compute::__internal::State::Gpu(state) => {
                                #gpu_name(state, threads).await
                            },
                            ::wgpu_compute::__internal::State::Cpu(state) => {
                                #cpu_name(state, threads)
                            },
                        }
                    }
                })

            } else {
                Ok(quote! {})
            }
        }).collect::<Result<Vec<proc_macro2::TokenStream>, GenerateError>>()?;

        Ok(quote! {
            #(#typedefs)*

            #(#constants)*

            #bindings

            #(#functions)*
        })
    }
}
