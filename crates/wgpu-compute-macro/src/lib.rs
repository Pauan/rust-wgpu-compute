use std::collections::HashSet;
use std::path::{PathBuf, Path};
use proc_macro2::{Span};
use syn::{Lit, ExprLit, Expr, Token, Ident, parse_macro_input};
use syn::punctuated::{Punctuated};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;

mod generate;


struct Field {
    name: Ident,
    value: Expr,
}

impl Parse for Field {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;

        input.parse::<Token![:]>()?;

        let value: Expr = input.parse()?;

        Ok(Self { name, value })
    }
}


struct File {
    span: Span,
    filename: String,
    path: PathBuf,
}


struct Module {
    source: String,
    path: String,
    span: Span,
    module: naga::Module,
}

impl Module {
    fn validate(&self) -> Result<naga::valid::ModuleInfo, syn::Error> {
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );

        validator.subgroup_stages(naga::valid::ShaderStages::all())
            .subgroup_operations(naga::valid::SubgroupOperationSet::all())
            .validate(&self.module).map_err(|e| {
                syn::Error::new(self.span, e.emit_to_string_with_path(&self.source, &self.path))
            })
    }

    fn to_source(&self, info: &naga::valid::ModuleInfo) -> Result<String, syn::Error> {
        naga::back::wgsl::write_string(
            &self.module,
            info,
            naga::back::wgsl::WriterFlags::EXPLICIT_TYPES,
        ).map_err(|e| {
            syn::Error::new(self.span, e.to_string())
        })
    }
}


struct Input {
    span: Span,
    root: PathBuf,
    file: Option<File>,
}

impl Input {
    fn new() -> Result<Self, syn::Error> {
        let span = Span::call_site();

        let mut root = span.local_file().ok_or_else(|| syn::Error::new(span, "Could not determine current file"))?;

        root.pop();

        let root = std::fs::canonicalize(root).map_err(|e| {
            syn::Error::new(span, e.to_string())
        })?;

        Ok(Self {
            span,
            root,
            file: None,
        })
    }


    fn load_file(&self) -> Result<Module, syn::Error> {
        let file = self.file.as_ref().ok_or_else(|| syn::Error::new(self.span, "Missing file option"))?;

        let rel_path = match file.path.strip_prefix(&self.root) {
            Ok(path) => path,
            Err(_) => &file.path,
        };

        let source = std::fs::read_to_string(&file.path).map_err(|e| {
            syn::Error::new(file.span, e.to_string())
        })?;

        let module = naga::front::wgsl::parse_str(&source).map_err(|e| {
            syn::Error::new(self.span, e.emit_to_string_with_path(&source, &rel_path))
        })?;

        Ok(Module {
            source,
            path: rel_path.to_string_lossy().into_owned(),
            span: file.span,
            module,
        })
    }


    fn to_tokens(&mut self) -> Result<proc_macro::TokenStream, syn::Error> {
        let module = self.load_file()?;

        let info = module.validate()?;

        let source = module.to_source(&info)?;

        let generate = generate::Generate {
            span: module.span,
            module: module.module,
        }.to_tokens(source)?;

        Ok(generate.into())
    }


    fn parse_file(&self, expr: Expr) -> Result<File, syn::Error> {
        match expr {
            Expr::Lit(ExprLit { ref attrs, lit: Lit::Str(ref s) }) => {
                if !attrs.is_empty() {
                    Err(syn::Error::new(expr.span(), "file cannot have attributes"))?;
                }

                let span = expr.span();

                let filename = s.value();

                let path = self.root.join(Path::new(filename.as_str()));

                let path = std::fs::canonicalize(path).map_err(|e| {
                    syn::Error::new(span, e.to_string())
                })?;

                Ok(File {
                    span,
                    filename,
                    path,
                })
            },
            x => {
                Err(syn::Error::new(x.span(), "file must be a string"))?
            },
        }

        /*match value {
            Expr::Array(array) => {
                if !array.attrs.is_empty() {
                    Err(syn::Error::new(array.span(), "files cannot have atributes"))?;
                }

                array.elems.into_iter().map(|expr: Expr| -> Result<File, syn::Error> {

                }).collect::<Result<Vec<_>, syn::Error>>()
            },
            x => Err(syn::Error::new(x.span(), "files must be an array"))?,
        }*/
    }
}

impl Parse for Input {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut output = Self::new()?;

        let mut seen = HashSet::new();

        let fields = Punctuated::<Field, Token![,]>::parse_terminated(input)?;

        for field in fields.into_iter() {
            let name = field.name.to_string();

            if !seen.insert(name.clone()) {
                Err(syn::Error::new(field.name.span(), format!("Duplicate option {}", name)))?;
            }

            match name.as_str() {
                "file" => {
                    output.file = Some(output.parse_file(field.value)?);
                },
                x => Err(syn::Error::new(field.name.span(), format!("Unknown option {}", x)))?,
            }
        }

        Ok(output)
    }
}


#[proc_macro]
pub fn import_wgpu_compute(
    tokens: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let mut input = parse_macro_input!(tokens as Input);

    match input.to_tokens() {
        Ok(tokens) => {
            println!("{}", tokens.to_string());
            tokens
        },
        Err(error) => error.into_compile_error().into(),
    }
}
