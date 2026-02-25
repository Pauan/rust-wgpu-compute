use proc_macro2::{Span};
use quote::{quote, format_ident};
use naga::Handle;
use naga::ir::*;


#[derive(Debug)]
struct Variable {
    name: String,
    space: AddressSpace,
    group: u32,
    binding: u32,
    ty: RustType,
}

fn scalar_to_rust(span: Span, ty: Scalar) -> Result<proc_macro2::TokenStream, syn::Error> {
    match ty.kind {
        ScalarKind::Sint => {
            match ty.width {
                1 => Ok(quote! { i8 }),
                2 => Ok(quote! { i16 }),
                4 => Ok(quote! { i32 }),
                8 => Ok(quote! { i64 }),
                16 => Ok(quote! { i128 }),
                _ => Err(syn::Error::new(span, "Integers cannot be larger than i128")),
            }
        },
        ScalarKind::Uint => {
            match ty.width {
                1 => Ok(quote! { u8 }),
                2 => Ok(quote! { u16 }),
                4 => Ok(quote! { u32 }),
                8 => Ok(quote! { u64 }),
                16 => Ok(quote! { u128 }),
                _ => Err(syn::Error::new(span, "Integers cannot be larger than u128")),
            }
        },
        ScalarKind::Float => {
            match ty.width {
                2 => Ok(quote! { f16 }),
                4 => Ok(quote! { f32 }),
                8 => Ok(quote! { f64 }),
                _ => Err(syn::Error::new(span, "Floats must be f16, f32, or f64")),
            }
        },
        ScalarKind::Bool => {
            Ok(quote! { bool })
        },
        _ => {
            Err(syn::Error::new(span, "Unknown scalar type"))
        },
    }
}


#[derive(Debug)]
struct RustType {
    ty: proc_macro2::TokenStream,
    inner: Option<proc_macro2::TokenStream>,
}

fn type_to_rust(span: Span, ty: Handle<Type>, module: &Module) -> Result<RustType, syn::Error> {
    let ty = module.types.get_handle(ty).unwrap();

    if let Some(name) = &ty.name {
        let name = format_ident!("{}", name);
        Ok(RustType {
            ty: quote! { #name },
            inner: None,
        })

    } else {
        match ty.inner {
            TypeInner::Array { base, size, stride: _ } => {
                let base = type_to_rust(span, base, module)?;

                match size {
                    ArraySize::Dynamic => {
                        Ok(RustType {
                            ty: quote! { ::std::vec::Vec<#base.ty> },
                            inner: Some(base.inner.unwrap_or(base.ty)),
                        })
                    },
                    ArraySize::Constant(size) => {
                        let size = size.get();
                        Ok(RustType {
                            ty: quote! { [#base.ty; #size] },
                            inner: Some(base.inner.unwrap_or(base.ty)),
                        })
                    },
                    ArraySize::Pending(_) => todo!(),
                }
            },
            TypeInner::Scalar(scalar) => {
                Ok(RustType {
                    ty: scalar_to_rust(span, scalar)?,
                    inner: None,
                })
            },
            TypeInner::Vector { size, scalar } => {
                let size = match size {
                    VectorSize::Bi => 2,
                    VectorSize::Tri => 3,
                    VectorSize::Quad => 4,
                };

                let scalar = scalar_to_rust(span, scalar)?;

                Ok(RustType {
                    ty: quote! { [#scalar; #size] },
                    inner: None,
                })
            },
            _ => {
                Err(syn::Error::new(span, "Unknown type"))
            },
        }
    }
}


fn typedef_to_rust(span: Span, handle: Handle<Type>, module: &Module) -> Result<proc_macro2::TokenStream, syn::Error> {
    let ty = module.types.get_handle(handle).unwrap();

    let name = format_ident!("{}", ty.name.as_ref().unwrap());

    match &ty.inner {
        // TODO handle span
        TypeInner::Struct { members, span: _ } => {
            // TODO handle offset
            let members = members.iter().map(|member| {
                let name = member.name.as_ref().ok_or_else(|| syn::Error::new(span, "Missing name on struct field"))?;
                let name = format_ident!("{}", name);

                let ty = type_to_rust(span, member.ty, module)?.ty;

                Ok(quote! { #name: #ty, })
            }).collect::<Result<Vec<_>, syn::Error>>()?;

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
                    #(#members)*
                }
            })
        },
        _ => {
            let ty = type_to_rust(span, handle, module)?.ty;

            Ok(quote! {
                pub type #name = #ty;
            })
        },
    }
}


pub struct Generate {
    pub span: Span,
    pub module: Module,
}

impl Generate {
    pub fn to_tokens(&self, source: String) -> Result<proc_macro2::TokenStream, syn::Error> {
        let mut typedefs: Vec<proc_macro2::TokenStream> = vec![];

        let mut variables: Vec<Vec<Variable>> = vec![];

        for (handle, ty) in self.module.types.iter() {
            if let Some(name) = &ty.name {
                typedefs.push(typedef_to_rust(self.span, handle, &self.module)?);
            }
        }

        for (_, variable) in self.module.global_variables.iter() {
            if let Some(ResourceBinding { group, binding }) = variable.binding {
                if let Some(name) = &variable.name {
                    while variables.len() < group {
                        variables.push(vec![]);
                    }

                    variables[group].push(Variable {
                        name: name.to_string(),
                        space: variable.space,
                        group,
                        binding,
                        ty: type_to_rust(self.span, variable.ty, &self.module)?,
                    });
                }
            }
        }

        let bindings = variables.iter().flatten().map(|input| {
            let name = format_ident!("{}", input.name);
            let ty = &input.ty;
            quote! { #name: #ty }
        }).collect::<Vec<_>>();

        let buffers = variables.iter().flatten().map(|input| {
            let name = format_ident!("{}", input.name);
            let ty = &input.ty;
            quote! { #name: ::wgpu_compute::Input<#ty> }
        }).collect::<Vec<_>>();

        let bind_group_layouts = variables.iter().map(|group| {
            let group = group.into_iter().map(|input| {
                let name = format_ident!("{}", input.name);
                let ty = &input.ty;
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
                    &::wgpu::BindGroupLayoutEntry {
                        binding: #binding,
                        visibility: ::wgpu::ShaderStages::COMPUTE,
                        ty: ::wgpu::BindingType::Buffer {
                            ty: #space,
                            has_dynamic_offset: false,
                            min_binding_size: ::std::option::Option::Some(
                                ::std::num::NonZeroU64::new(::std::mem::size_of::<#ty>()).unwrap()
                            ),
                        },
                        count: ::std::option::Option::None,
                    },
                }
            }).collect::<Vec<_>>();

            quote! {
                &[#(#group,)*]
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
                fn gpu_layout(gpu: &::wgpu_compute::Gpu) -> &::wgpu_compute::__internal::GpuLayout {
                    ::std::thread_local! {
                        static GPU_LAYOUT: ::std::cell::OnceCell<::wgpu_compute::__internal::GpuLayout> =
                            ::std::cell::OnceCell::new();
                    }

                    GPU_LAYOUT.with(|gpu_layout| {
                        gpu_layout.get_or_init(|| {
                            ::wgpu_compute::__internal::GpuLayout::new(
                                gpu,
                                #source,
                                &[
                                    #(#bind_group_layouts)*
                                ],
                            )
                        })
                    })
                }

                fn bind_group(&self) -> &[::wgpu::BindGroupEntry] {
                    todo!();
                    /*&[
                        ::wgpu::BindGroupEntry {
                        },
                    ]*/
                }
            }

            impl ::wgpu_compute::ToBuffers for Bindings {
                type Output = Buffers;

                fn to_buffers(&self, gpu: &::wgpu_compute::Gpu) -> Self::Output {
                    todo!();
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

                            let gpu_layout = Buffers::gpu_layout(&state.gpu);

                            GPU_FN.with(|gpu_fn| {
                                let gpu_fn = gpu_fn.get_or_init(|| {
                                    ::wgpu_compute::__internal::GpuFn::new(
                                        &state.gpu,
                                        gpu_layout,
                                        [#x, #y, #z],
                                        #entry_name,
                                    )
                                });

                                let mut encoder = ::wgpu_compute::__internal::command_encoder(
                                    &state.gpu,
                                    gpu_layout,
                                    gpu_fn,
                                    threads,
                                    state.buffers.bind_group(),
                                );

                                for (input, output) in state.copy_buffers {
                                    ::wgpu_compute::__internal::copy_input_to_output(&mut encoder, &input, &output);
                                }

                                ::wgpu_compute::__internal::wait(&state.gpu, encoder)
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
        }).collect::<Result<Vec<proc_macro2::TokenStream>, syn::Error>>()?;

        Ok(quote! {
            #(#typedefs)*

            #bindings

            #(#functions)*
        })
    }
}
