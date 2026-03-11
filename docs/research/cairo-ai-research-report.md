# Cairo Language AI Applications Research Report

## Executive Summary

This research report provides a comprehensive analysis of the current usages of Cairo programming language in artificial intelligence contexts, with particular focus on zero-knowledge machine learning (ZKML) applications and the emerging ecosystem of provable AI tools built on the StarkWare stack. The investigation reveals a vibrant and rapidly evolving landscape of AI implementations within the Cairo ecosystem, ranging from neural network implementations to comprehensive provable machine learning frameworks. The S-two prover represents a significant advancement in enabling AI verification capabilities within the Cairo ecosystem, offering unprecedented performance for zero-knowledge proof generation related to machine learning workloads.

## 1. Introduction and Background

Cairo is a Turing-complete programming language developed by StarkWare that serves as the native language for StarkNet, Ethereum's Layer 2 scaling solution. Originally designed for creating provable programs for general computation, Cairo has evolved to become a powerful platform for zero-knowledge proof generation, with significant implications for artificial intelligence applications. The language abstracts away the complex cryptography and mathematics involved in proof generation, allowing developers to focus on building applications while the underlying STARK (Scalable Transparent ARguments of Knowledge) cryptographic infrastructure handles the proving mechanism.

The intersection of Cairo with AI represents a relatively new but rapidly developing field, driven primarily by the need for verifiable and privacy-preserving machine learning. Zero-knowledge machine learning (ZKML) enables the verification of ML model execution without revealing the underlying model weights or input data, creating new possibilities for trustless AI applications. This research investigates the current state of AI implementations within the Cairo ecosystem, examines the feature enabling aspects of the S-two prover, and provides a comprehensive feature matrix for comparative analysis.

## 2. Cairo AI Ecosystem Overview

### 2.1 Foundational AI Frameworks

The Cairo ecosystem has developed several specialized frameworks and tools for AI and machine learning applications. The most prominent among these is Orion, an open-source, community-driven framework dedicated to provable machine learning. Developed by Gizatech, Orion provides essential components for building verifiable ML models and implements an ONNX Runtime in Cairo 1.0 for executing machine learning inference with cryptographic proof verification through STARKs.

Orion represents a significant milestone in democratizing access to verifiable machine learning, enabling developers to train models in mainstream frameworks such as TensorFlow or PyTorch and then execute them with verifiable inference on StarkNet. The framework leverages the ONNX (Open Neural Network Exchange) format as a universal intermediary, ensuring compatibility with the broader deep learning ecosystem. This approach allows organizations to maintain their existing ML workflows while gaining the ability to generate cryptographic proofs of correct inference execution.

The framework is structured around three primary components: the Framework itself providing building blocks for verifiable machine learning models, the Hub offering a curated collection of community-built ML models and demonstration spaces, and the Academy providing educational resources and tutorials for developers seeking to build ValidityML applications. As of March 2025, the original Orion project has been archived, with the development team transitioning to work on LuminAIR, a new zkML framework based on custom AIR (Algebraic Intermediate Representation) proven with the S-two Prover.

### 2.2 Neural Network Implementations

Several notable neural network implementations exist within the Cairo ecosystem, demonstrating the practical applicability of machine learning models on the platform. The neural-network-cairo project provides a complete neural network implementation from scratch for MNIST digit classification, written entirely in Cairo 1.0. This implementation features a two-layer architecture with 784 input units corresponding to the 28×28 pixel images, a hidden layer with 10 units using ReLU activation, and an output layer with 10 units using softmax activation for digit classification.

The implementation includes sophisticated features such as 8-bit weight quantization based on ONNX quantization standards, various data structure implementations including vectors, matrices, and tensors with associated operations, forward propagation capabilities, and methods for loading pre-trained weights from TensorFlow models into Cairo neural networks. This project demonstrates the feasibility of running practical machine learning inference on blockchain infrastructure while maintaining verifiability guarantees.

Additional AI demonstrations within the ecosystem include Tic-Tac-Stark, which implements a provable Tic-Tac-Toe AI model using Orion and Cairo, and drive-ai, a self-driving car AI project built on the Dojo game engine. These projects, while more experimental in nature, demonstrate the versatility of Cairo for different AI application domains and serve as proof-of-concept implementations for more sophisticated future applications.

## 3. Zero-Knowledge Machine Learning Applications

### 3.1 Cairo Verifier and ZKML Capabilities

The Cairo Verifier deployed on StarkNet represents a transformative advancement for zero-knowledge machine learning applications. The verifier enables STARK proofs to be verified directly on StarkNet smart contracts, providing substantially cheaper verification costs compared to Ethereum mainnet verification. This cost efficiency is essential for making ZKML applications economically viable, as the computational overhead of proof verification has historically been a significant barrier to adoption.

The technical architecture of the Cairo Verifier features a single contract design, unlike Ethereum's Solidity verifier which is split across dozens of separate contracts. This monolith architecture improves auditability and reduces the complexity of integration for developers. The verifier supports bootloader programs that reduce verifier complexity without revealing entire bytecode, flexible hash functions including Poseidon, Blake, and Keccak for optimization trade-offs between prover and verifier costs, and various Cairo layouts with different builtin combinations.

The enablement of client-side proving represents a particularly powerful capability for ZKML applications. Machine learning computations can be proven locally on client devices and verified on-chain, creating a privacy-preserving workflow where sensitive input data never leaves the client while still providing cryptographic guarantees of correct execution. This architecture supports the verification of model outputs without revealing model weights, enabling scenarios such as proving that a credit risk assessment was generated by a specific model without exposing the proprietary model parameters.

### 3.2 ZKML Use Cases and Applications

Zero-knowledge machine learning enabled by Cairo encompasses a broad range of practical applications across multiple industries. The technology enables verification of AI model outputs in decentralized contexts, allowing smart contracts to depend on AI-generated decisions while maintaining cryptographic certainty about the correctness of those decisions. This capability is particularly valuable for DeFi applications requiring credit scoring, risk assessment, or automated decision-making that benefits from verifiability guarantees.

Privacy-preserving inference represents another significant application domain, where ZKML enables proving the correctness of ML inference without revealing the underlying input data. This capability addresses regulatory compliance requirements in sectors such as healthcare and finance, where sensitive data processing must be combined with strong privacy guarantees. The technology also enables verifiable random forests and other ensemble methods, where the integrity of complex voting-based predictions can be cryptographically verified.

The integration with the broader StarkNet ecosystem enables sophisticated multi-party computation scenarios where different participants can contribute to ML inference while maintaining confidentiality over their respective inputs. This capability opens possibilities for collaborative machine learning on blockchain infrastructure, where multiple organizations can jointly evaluate models on combined datasets without exposing individual data contributions.

## 4. S-two Prover and AI Feature Enablement

### 4.1 Technical Architecture and Performance

The S-two prover (stwo-cairo) represents StarkWare's next-generation zero-knowledge proving system, described as the fastest prover in the world. The prover is fully open-source and written in Rust, implementing the Circle STARK protocol—a breakthrough proving system over 31-bit chip-friendly Mersenne prime field. The technical architecture supports multiple hardware backends including CPU, SIMD, and GPU, with WebGPU and WASM compilation for in-browser proving capabilities scheduled for future release.

The performance characteristics of S-two are particularly relevant for AI applications, with benchmarks demonstrating 28X faster performance than Risc0 precompile on Keccak chain and 39X faster than SP1 precompile on equivalent benchmarks. The prover achieves 10-30X performance gains depending on the specific task compared to alternative solutions. These performance improvements are attributable to the use of high-level general-purpose language (Cairo) rather than low-level hand-tuned circuits, combined with the mathematical innovations of the Circle STARK protocol.

The S-two prover architecture supports recursive proving out of the box, enabling complex proof compositions that are essential for sophisticated AI verification workflows. The compatible Cairo programming language provides a familiar development environment for developers building AI applications, while the prover's client-side capabilities enable practical deployment scenarios where proof generation occurs on consumer hardware including phones, laptops, and browser tabs.

### 4.2 AI-Specific Capabilities

S-two includes specific features designed to address AI verification needs, categorized into four primary capability areas. Model authenticity verification enables proving that a known model produced a given output, addressing concerns about adversarial substitution of ML models in production systems. This capability ensures that on-chain applications can verify they are interacting with the intended model rather than a modified version.

Input integrity verification provides mechanisms to prove what inputs were used in a computation without revealing those inputs, essential for privacy-preserving AI applications. This capability enables scenarios where the existence and correctness of specific input data can be demonstrated without exposing the actual data values, creating powerful primitives for privacy-sensitive applications.

zkML inference enables running provable inference locally with proofs verified on-chain, bringing zero-knowledge capabilities to standard machine learning workflows. The practical feasibility of generating proofs of ML inference on real-world hardware represents a significant advancement for the field, making it economically viable to deploy verifiable AI in production systems. AI automation capabilities enable triggering on-chain actions based on verifiable AI outcomes, closing the loop between off-chain AI inference and on-chain execution.

### 4.3 Integration with Cairo Ecosystem

The integration of S-two with the broader Cairo ecosystem creates a comprehensive platform for AI application development. The prover is accessible through the Scarb build tool via the `scarb prove` command (requires Scarb version 2.10.0 or later), providing a seamless developer experience for building and proving Cairo programs. The dual crate architecture separates prover and verifier implementations, enabling optimized deployment strategies based on specific application requirements.

The availability of S-two as an open-source project (272 stars, 60 forks, 1,199 commits) ensures community engagement and transparency in the proving infrastructure. The expected on-chain verifier deployment on StarkNet by the end of 2025 will further enhance the accessibility of these capabilities for production AI applications, enabling direct on-chain verification of proofs generated by the S-two prover.

## 5. Feature Matrix Analysis

The following feature matrix provides a comparative analysis of the key AI-related capabilities across the Cairo ecosystem components:

| Feature Category         | Cairo Language  | Orion Framework    | S-two Prover  | Cairo Verifier    |
| ------------------------ | --------------- | ------------------ | ------------- | ----------------- |
| **ML Framework**         |                 |                    |               |                   |
| Neural Network Support   | Basic           | Advanced           | N/A           | N/A               |
| ONNX Runtime             | Via External    | Native             | N/A           | N/A               |
| Model Training           | Not Supported   | Limited            | N/A           | N/A               |
| Pre-trained Model Import | Via Manual Port | Automatic (ONNX)   | N/A           | N/A               |
| **Proving Capabilities** |                 |                    |               |                   |
| STARK Proof Generation   | Native          | Via Framework      | Native        | N/A               |
| ZK Proof Support         | Native          | Native             | Native        | Verification Only |
| Recursive Proving        | Via Libraries   | Via Libraries      | Native        | Limited           |
| Client-side Proving      | Not Native      | Not Native         | Native        | N/A               |
| **AI Verification**      |                 |                    |               |                   |
| Model Authenticity       | Manual          | Via Framework      | Native        | Supported         |
| Input Integrity          | Manual          | Via Framework      | Native        | Supported         |
| Output Verification      | Manual          | Via Framework      | Via Proof     | Supported         |
| Privacy Preservation     | Manual          | Via Framework      | Native        | Supported         |
| **Performance**          |                 |                    |               |                   |
| Proof Generation Speed   | Standard        | Standard           | 28-39X Faster | N/A               |
| Verification Cost        | High            | Medium             | Low           | Low (L2)          |
| Hardware Acceleration    | No              | No                 | CPU/SIMD/GPU  | N/A               |
| Browser Support          | No              | No                 | Coming Soon   | N/A               |
| **Development**          |                 |                    |               |                   |
| Language                 | Cairo 1.0       | Cairo 1.0          | Rust/Cairo    | Cairo             |
| Framework Integration    | N/A             | TensorFlow/PyTorch | N/A           | N/A               |
| Documentation            | Comprehensive   | Good               | Good          | Good              |
| Maintenance Status       | Active          | Archived           | Active        | Active            |

### 5.1 Feature Matrix Interpretation

The feature matrix reveals distinct specializations among the ecosystem components. Cairo language provides the foundational programming model and native STARK proof generation capabilities, serving as the substrate upon which higher-level AI tools are built. The language itself supports basic neural network operations through manual implementation but does not provide integrated ML framework capabilities.

The Orion framework delivers the most comprehensive ML-specific feature set, including native ONNX runtime support, automatic model import from standard frameworks, and built-in mechanisms for verifiable inference. However, its archived status as of March 2025 raises questions about long-term maintenance and suggests that the ecosystem may be transitioning toward newer solutions.

The S-two prover represents the performance leader in the ecosystem, providing native support for all key AI verification capabilities including model authenticity, input integrity, and privacy-preserving inference. Its hardware acceleration support and client-side proving capabilities position it as the preferred solution for production AI applications requiring high throughput and low latency.

The Cairo Verifier provides essential on-chain verification capabilities at substantially reduced costs compared to Ethereum mainnet alternatives. Its support for client-side proving enables the privacy-preserving ZKML workflows that are essential for many practical applications.

## 6. Practical Applications and Use Cases

### 6.1 Current Implementations

The practical applications of Cairo-based AI span multiple domains and complexity levels. The MNIST digit recognition implementation demonstrates the feasibility of running conventional machine learning inference on blockchain infrastructure, providing a template for more sophisticated image classification applications. The Tic-Tac-Toe AI demonstrates game AI with provable correctness guarantees, illustrating how AI decision-making can be verified in adversarial settings.

The Dojo-based drive-ai project extends AI applications into simulation and gaming domains, leveraging the provable game engine to create verifiable autonomous agent behaviors. These early implementations serve as proof-of-concept demonstrations that establish the technical viability of more ambitious future applications.

### 6.2 Emerging Applications

The combination of S-two prover capabilities with Cairo's programming model enables several emerging application categories. Verifiable AI agents can operate autonomously on-chain while providing cryptographic proof of their decision-making processes, enabling trustless automation of complex financial and organizational tasks. The integration with AI agents enables on-chain actions to be triggered based on verifiable AI outcomes, creating closed-loop systems where off-chain intelligence drives on-chain execution.

Proof of humanity and identity applications leverage ZKML capabilities to verify human characteristics without revealing biometric data, addressing concerns about bot proliferation while maintaining privacy. Age verification and zkKYC (zero-knowledge Know Your Customer) represent additional application domains where AI classification must be combined with privacy-preserving verification mechanisms.

Decentralized identity systems benefit from ZKML capabilities through verifiable credentials that demonstrate possession of certain attributes without revealing the underlying evidence. Proof of uniqueness systems enable demonstration of personhood without revealing specific identity, addressing Sybil attack concerns in governance and allocation systems while preserving anonymity.

## 7. Conclusion

The Cairo ecosystem has developed a comprehensive suite of tools and frameworks for AI applications, with particular strength in zero-knowledge machine learning and verifiable AI systems. The combination of the Cairo programming language, Orion framework (and its successor LuminAIR), S-two prover, and Cairo Verifier creates a complete platform for developing, deploying, and verifying AI applications on blockchain infrastructure.

The S-two prover represents a significant advancement in enabling practical AI verification, with performance characteristics that make client-side proof generation viable on consumer hardware. Its specific capabilities for model authenticity, input integrity, and privacy-preserving inference directly address the requirements of production AI applications.

The ecosystem continues to evolve, with the transition from Orion to LuminAIR indicating ongoing development activity. The expected deployment of on-chain verification for S-two proofs on StarkNet by the end of 2025 will further enhance the accessibility of these capabilities for mainstream applications.

For organizations seeking to build verifiable or privacy-preserving AI applications on blockchain infrastructure, the Cairo ecosystem provides a mature and well-documented platform. The key considerations include selecting appropriate tools based on specific requirements (performance, privacy, verification needs), monitoring the transition from Orion to LuminAIR for updated capabilities, and leveraging client-side proving capabilities for privacy-sensitive applications.

## 8. References

The following sources were consulted during the preparation of this research report:

- StarkWare Industries. "Introducing S-two: The fastest prover for real-world ZK applications." https://starkware.co/blog/s-two-prover/
- StarkNet. "Meet the Cairo Verifier: Customizable L3 Appchains on Starknet." https://www.starknet.io/blog/meet-the-cairo-verifier/
- Gizatech. "Orion: ONNX Runtime in Cairo 1.0 for verifiable ML." https://github.com/gizatechxyz/orion
- StarkWare Industries. "Cairo Use Cases." https://starkware.co/use-cases/
- StarkWare Libraries. "starkware-libs/stwo-cairo GitHub Repository." https://github.com/starkware-libs/stwo-cairo
- Algaba, F. "Neural Network implementation for MNIST in Cairo." https://github.com/franalgaba/neural-network-cairo
- Keep Starknet Strange. "Awesome Starknet Repository." https://github.com/keep-starknet-strange/awesome-starknet
- Cairo Language Documentation. https://www.cairo-lang.org/
- StarkNet Documentation. https://www.starknet.io/

---

**Report prepared by:** MiniMax Agent
**Date:** March 2, 2026
