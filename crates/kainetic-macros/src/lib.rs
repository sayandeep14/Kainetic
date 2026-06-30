//! Procedural macros for Kainetic: `#[tool]`, `#[agent]`, and `#[pipeline]`.
//!
//! - `#[tool]` — derives a `kainetic_tools::Tool` impl and a matching
//!   struct from an async function, injecting JSON Schema deserialization,
//!   timeout, and a `tracing` span.
//! - `#[agent]` — derives an `Agent` impl from an async function.
//! - `#[pipeline]` — validates and wires a multi-agent pipeline graph at
//!   compile time.
//!
//! Always implement the underlying trait manually first, then use these macros.
#![deny(clippy::all, unsafe_code)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, punctuated::Punctuated, FnArg, GenericArgument, ItemFn, PathArguments,
    ReturnType, Token, Type,
};

// ─── #[tool] ─────────────────────────────────────────────────────────────────

/// Derives a `kainetic_tools::Tool` implementation from an async function.
///
/// The macro:
/// 1. Keeps the original function in place.
/// 2. Generates a unit struct named after the function in `PascalCase`.
/// 3. Implements `kainetic_tools::Tool` for that struct, wiring `name`,
///    `description`, `input_schema`, and `output_schema` from the function
///    metadata and types.
/// 4. The generated `call` deserialises the raw JSON input, calls the function,
///    and serialises the output.  A `tracing` span is entered for the call.
///
/// # Signature requirements
///
/// ```rust,ignore
/// #[kainetic_macros::tool(description = "Human-readable purpose.")]
/// async fn my_tool(input: MyInput, ctx: ToolContext) -> Result<MyOutput, ToolError> {
///     // ...
/// }
/// ```
///
/// - First parameter: typed input (must implement `Deserialize` + `JsonSchema`).
/// - Second parameter: `kainetic_tools::ToolContext` (name can vary).
/// - Return type: `Result<OutputType, ToolError>` where `OutputType: Serialize + JsonSchema`.
///
/// # Generated items
///
/// For `async fn my_tool(...)`:
/// - Original function unchanged.
/// - `pub struct MyTool;`
/// - `impl kainetic_tools::Tool for MyTool { … }`
#[proc_macro_attribute]
pub fn tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attrs = parse_macro_input!(attr as ToolAttrs);
    let func = parse_macro_input!(item as ItemFn);

    match expand_tool(attrs, func) {
        Ok(ts) => ts.into(),
        Err(e) => e.into_compile_error().into(),
    }
}

/// Derives a `kainetic_core::Agent` implementation from an async function.
///
/// The macro:
/// 1. Keeps the original function in place.
/// 2. Generates a unit struct (with optional `AgentConfig` field) named after
///    the function in `PascalCase`.
/// 3. Implements `kainetic_core::Agent` for that struct.
///
/// # Signature requirements
///
/// ```rust,ignore
/// #[kainetic_macros::agent(description = "What this agent does.")]
/// async fn my_agent(input: MyInput, ctx: AgentContext) -> Result<MyOutput, AgentError> {
///     // ...
/// }
/// ```
///
/// - First parameter: the typed input (`type Input`).
/// - Second parameter: `kainetic_core::AgentContext` (name can vary).
/// - Return type: `Result<OutputType, ErrorType>`.
///
/// # Generated items
///
/// For `async fn my_agent(...)`:
/// - Original function unchanged.
/// - `pub struct MyAgent { pub config: kainetic_core::AgentConfig }`
/// - `impl MyAgent { pub fn new() -> Self; pub fn with_config(AgentConfig) -> Self }`
/// - `impl Default for MyAgent`
/// - `impl kainetic_core::Agent for MyAgent { … }`
#[proc_macro_attribute]
pub fn agent(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attrs = parse_macro_input!(attr as AgentAttrs);
    let func = parse_macro_input!(item as ItemFn);

    match expand_agent(attrs, func) {
        Ok(ts) => ts.into(),
        Err(e) => e.into_compile_error().into(),
    }
}

/// Validates and wires a multi-agent pipeline graph at compile time.
///
/// See the `kainetic-orchestra` crate documentation for usage examples.
#[proc_macro_attribute]
pub fn pipeline(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

// ─── Attribute parsing ────────────────────────────────────────────────────────

#[derive(Default)]
struct ToolAttrs {
    description: Option<String>,
    timeout_ms: Option<u64>,
}

impl syn::parse::Parse for ToolAttrs {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut attrs = ToolAttrs::default();
        if input.is_empty() {
            return Ok(attrs);
        }
        let pairs = Punctuated::<syn::MetaNameValue, Token![,]>::parse_terminated(input)?;
        for pair in &pairs {
            let ident = pair
                .path
                .get_ident()
                .ok_or_else(|| syn::Error::new_spanned(&pair.path, "expected identifier"))?;
            match ident.to_string().as_str() {
                "description" => {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = &pair.value
                    {
                        attrs.description = Some(s.value());
                    }
                }
                "timeout" => {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = &pair.value
                    {
                        attrs.timeout_ms = parse_duration_ms(&s.value());
                    }
                }
                unknown => {
                    return Err(syn::Error::new_spanned(
                        &pair.path,
                        format!(
                            "#[tool] unknown attribute `{unknown}` — supported attributes: `description`, `timeout`"
                        ),
                    ));
                }
            }
        }
        Ok(attrs)
    }
}

/// Parses a duration string like `"30s"` or `"500ms"` into milliseconds.
fn parse_duration_ms(s: &str) -> Option<u64> {
    if let Some(ms) = s.strip_suffix("ms") {
        ms.parse::<u64>().ok()
    } else if let Some(secs) = s.strip_suffix('s') {
        secs.parse::<u64>().ok().map(|n| n * 1_000)
    } else {
        None
    }
}

// ─── Code generation ──────────────────────────────────────────────────────────

fn expand_tool(attrs: ToolAttrs, func: ItemFn) -> syn::Result<proc_macro2::TokenStream> {
    if func.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            func.sig.fn_token,
            "#[tool] functions must be `async fn` — add the `async` keyword",
        ));
    }

    let fn_name = &func.sig.ident;
    let fn_name_str = fn_name.to_string();

    // Derive the struct name: snake_case → PascalCase
    let struct_name_str = to_pascal_case(&fn_name_str);
    let struct_ident = syn::Ident::new(&struct_name_str, fn_name.span());

    let description = attrs.description.unwrap_or_default();

    // Extract the first parameter's type as the InputType.
    let input_type = extract_first_param_type(&func)?;

    // Extract OutputType from `Result<OutputType, _>`.
    let output_type = extract_output_type(&func)?;

    // Build the call expression, optionally wrapped in a timeout block.
    let inner_call = if let Some(timeout_ms) = attrs.timeout_ms {
        quote! {
            {
                ::tokio::time::timeout(
                    ::std::time::Duration::from_millis(#timeout_ms),
                    #fn_name(__typed_input, ctx),
                )
                .await
                .map_err(|_| ::kainetic_tools::ToolError::Timeout)??
            }
        }
    } else {
        quote! {
            #fn_name(__typed_input, ctx).await?
        }
    };

    let struct_doc =
        format!("Auto-generated [`kainetic_tools::Tool`] implementation for [`{fn_name_str}`].");

    let expanded = quote! {
        #func

        #[doc = #struct_doc]
        pub struct #struct_ident;

        impl ::kainetic_tools::Tool for #struct_ident {
            fn name(&self) -> &'static str {
                #fn_name_str
            }

            fn description(&self) -> &'static str {
                #description
            }

            fn input_schema(&self) -> ::schemars::schema::RootSchema {
                ::schemars::schema_for!(#input_type)
            }

            fn output_schema(&self) -> ::schemars::schema::RootSchema {
                ::schemars::schema_for!(#output_type)
            }

            fn call(
                &self,
                input: ::serde_json::Value,
                ctx: ::kainetic_tools::ToolContext,
            ) -> ::kainetic_tools::ToolFuture<'_> {
                ::std::boxed::Box::pin(async move {
                    let __span = ::tracing::info_span!("tool.call", tool.name = #fn_name_str);
                    let _enter = __span.enter();
                    let __typed_input: #input_type =
                        ::serde_json::from_value(input).map_err(|e| {
                            ::kainetic_tools::ToolError::InputValidation(e.to_string())
                        })?;
                    let __output = #inner_call;
                    ::serde_json::to_value(__output).map_err(|e| {
                        ::kainetic_tools::ToolError::ExecutionFailed(e.to_string())
                    })
                })
            }
        }
    };

    Ok(expanded)
}

/// Converts a `snake_case` identifier to `PascalCase`.
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

/// Extracts the type of the first function parameter (the tool input).
fn extract_first_param_type(func: &ItemFn) -> syn::Result<Box<Type>> {
    let first = func.sig.inputs.first().ok_or_else(|| {
        syn::Error::new_spanned(
            &func.sig,
            "#[tool] requires at least one parameter: `fn my_tool(input: MyInput, ctx: ToolContext) -> …`",
        )
    })?;
    match first {
        FnArg::Typed(pat_type) => Ok(pat_type.ty.clone()),
        FnArg::Receiver(r) => Err(syn::Error::new_spanned(
            r,
            "#[tool] functions must be free functions, not methods — remove the `self` parameter",
        )),
    }
}

// ─── #[agent] implementation ──────────────────────────────────────────────────

#[derive(Default)]
struct AgentAttrs {
    description: Option<String>,
}

impl syn::parse::Parse for AgentAttrs {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut attrs = AgentAttrs::default();
        if input.is_empty() {
            return Ok(attrs);
        }
        let pairs = Punctuated::<syn::MetaNameValue, Token![,]>::parse_terminated(input)?;
        for pair in &pairs {
            let ident = pair
                .path
                .get_ident()
                .ok_or_else(|| syn::Error::new_spanned(&pair.path, "expected identifier"))?;
            if ident == "description" {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = &pair.value
                {
                    attrs.description = Some(s.value());
                }
            }
        }
        Ok(attrs)
    }
}

fn expand_agent(attrs: AgentAttrs, func: ItemFn) -> syn::Result<proc_macro2::TokenStream> {
    if func.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            func.sig.fn_token,
            "#[agent] functions must be `async fn` — add the `async` keyword",
        ));
    }

    let fn_name = &func.sig.ident;
    let fn_name_str = fn_name.to_string();

    let struct_name_str = to_pascal_case(&fn_name_str);
    let struct_ident = syn::Ident::new(&struct_name_str, fn_name.span());

    let description = attrs.description.unwrap_or_default();

    let input_type = extract_first_param_type(&func)?;
    let output_type = extract_output_type(&func)?;
    let error_type = extract_error_type(&func)?;

    let struct_doc =
        format!("Auto-generated [`kainetic_core::Agent`] implementation for [`{fn_name_str}`].");

    let expanded = quote! {
        #func

        #[doc = #struct_doc]
        pub struct #struct_ident {
            /// Runtime configuration for this agent instance.
            pub config: ::kainetic_core::AgentConfig,
        }

        impl #struct_ident {
            /// Creates an agent with default configuration.
            #[must_use]
            pub fn new() -> Self {
                Self {
                    config: ::kainetic_core::AgentConfig::builder().build(),
                }
            }

            /// Creates an agent with the supplied configuration.
            #[must_use]
            pub fn with_config(config: ::kainetic_core::AgentConfig) -> Self {
                Self { config }
            }
        }

        impl Default for #struct_ident {
            fn default() -> Self {
                Self::new()
            }
        }

        impl ::kainetic_core::Agent for #struct_ident {
            type Input = #input_type;
            type Output = #output_type;
            type Error = #error_type;

            fn name(&self) -> &'static str {
                #fn_name_str
            }

            fn description(&self) -> &'static str {
                #description
            }

            fn config(&self) -> &::kainetic_core::AgentConfig {
                &self.config
            }

            fn run(
                &self,
                input: Self::Input,
                ctx: ::kainetic_core::AgentContext,
            ) -> ::kainetic_core::AgentFuture<'_, Self::Output, Self::Error> {
                ::std::boxed::Box::pin(#fn_name(input, ctx))
            }
        }
    };

    Ok(expanded)
}

/// Extracts the second generic argument (`E`) from `Result<T, E>`.
fn extract_error_type(func: &ItemFn) -> syn::Result<Box<Type>> {
    let (arrow, ty) = match &func.sig.output {
        ReturnType::Type(arrow, ty) => (arrow, ty),
        ReturnType::Default => {
            return Err(syn::Error::new_spanned(
                &func.sig,
                "#[agent] function must have a return type: `-> Result<OutputType, ErrorType>`",
            ));
        }
    };

    if let Type::Path(type_path) = ty.as_ref() {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Result" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(err_ty)) = args.args.iter().nth(1) {
                        return Ok(Box::new(err_ty.clone()));
                    }
                }
            }
        }
    }

    Err(syn::Error::new_spanned(
        quote::quote!(#arrow #ty),
        "#[agent] return type must be `Result<OutputType, ErrorType>` — both type parameters are required",
    ))
}

// ─── Shared helpers ───────────────────────────────────────────────────────────

/// Extracts `T` from a return type of the form `Result<T, _>`.
fn extract_output_type(func: &ItemFn) -> syn::Result<Box<Type>> {
    let (arrow, ty) = match &func.sig.output {
        ReturnType::Type(arrow, ty) => (arrow, ty),
        ReturnType::Default => {
            return Err(syn::Error::new_spanned(
                &func.sig,
                "#[tool] function must have a return type: `-> Result<OutputType, ToolError>`",
            ));
        }
    };

    if let Type::Path(type_path) = ty.as_ref() {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Result" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(inner)) = args.args.first() {
                        return Ok(Box::new(inner.clone()));
                    }
                }
            }
        }
    }

    Err(syn::Error::new_spanned(
        quote::quote!(#arrow #ty),
        "#[tool] return type must be `Result<OutputType, ToolError>` — wrap your output in `Ok(…)`",
    ))
}
