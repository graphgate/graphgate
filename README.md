# GraphGate

<div align="center">
  <!-- CI -->
  <img src="https://gitlab.com/oss47/graphgate/badges/master/pipeline.svg" />
</div>

GraphGate is a high-performance [Apollo Federation](https://www.apollographql.com/apollo-federation) compatible implementation written in Rust. It enables you to build a distributed GraphQL architecture by composing multiple services into a unified schema.

## Features

- **High Performance**: Built in Rust for optimal memory usage and execution speed
- **Federation Compatibility**: Compatible with Apollo Federation specifications
- **Directive-Based Schema Composition**: Uses GraphQL directives to compose schemas from multiple services
- **Optimized Query Planning**: Creates efficient query execution plans to minimize service calls
- **Modular Architecture**: Component-based design for easy maintenance and extensibility

## Supported Directives

GraphGate implements a range of Apollo Federation directives through a modular [Directive Registry Pattern](crates/planner/src/builder/directive_handlers/README.md):

### Currently Implemented Directives

- **@key**: Marks a type as an entity and defines the fields that uniquely identify it across services
- **@requires**: Specifies fields from another service that must be fetched before resolving a field
- **@provides**: Indicates that a field can fetch specific subfields of an entity from another service
- **@tag**: Adds metadata to fields or types to enable runtime behavior modifications
- **@external**: Marks a field as defined in another service
- **@shareable**: Marks a type as shareable across subgraphs
- **@link**: Links definitions from an external specification to the schema

## Architecture

GraphGate is organized into several crates:

- **schema**: Handles schema composition and validation of federated services
- **planner**: Creates efficient query execution plans using directive information
- **handler**: Manages request handling and communication with services
- **validation**: Performs schema validation against Federation specifications

The planner uses a modular Directive Registry Pattern where each directive handler implements the `DirectiveHandlerTrait` interface. This makes the codebase more maintainable and allows for easier addition of new directive handlers.

## Getting Started

### Running with Docker

```bash
docker run -p 4000:4000 graphgate/graphgate
```

### Examples

Check the `examples` directory for sample services showing how to use GraphGate with various directive combinations:

- **accounts.rs**: User authentication and account management service with entity resolution
- **products.rs**: Product catalog service demonstrating entity relationships
- **reviews.rs**: Product review service showcasing more complex entity relations and directive usage

## Current Limitations

- **Federation v2 Features**: Some advanced Federation v2 features are still in development
- **Subscription Support**: Limited support for GraphQL subscriptions across federated services
- **Custom Scalars**: Limited support for custom scalar types across services
- **Error Handling**: Error propagation from subgraphs could be improved
- **Advanced Query Optimization**: Some complex query patterns could benefit from further optimization
- **Missing Directive Handlers**: Several directives like @inaccessible are defined in the schema but not yet implemented

## Contributing

Contributions are welcome! GraphGate is actively being improved, and we appreciate help with:

- Implementing additional directive handlers
- Improving performance optimizations
- Enhancing error handling and reporting
- Expanding test coverage
- Improving documentation

## License

This project is licensed under the MIT/Apache-2.0 License - see the LICENSE file for details.
