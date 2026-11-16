# Render delta
## ADDED Requirements
### Requirement: Opaque fallback by default
The framework MUST create an Opaque fallback material when no texture array or alpha override is supplied.
#### Scenario: Solid default terrain
- **Given** default chunk material configuration and no texture array
- **When** material startup runs
- **Then** the shared StandardMaterial SHALL use AlphaMode::Opaque.
### Requirement: Explicit fallback alpha policy
Consumers MAY configure the shared fallback AlphaMode. The framework MUST forward that value without changing other configured properties.
#### Scenario: Custom translucent palette
- **Given** an explicit Blend configuration
- **When** material startup runs without a texture array
- **Then** the shared material SHALL use Blend.
### Requirement: Preserve air and texture rendering
DefaultVoxel solids MUST retain alpha 1 and air MUST emit no mesh geometry at either supported LOD. Fallback alpha policy MUST NOT affect the texture-array material.
#### Scenario: Empty chunk
- **Given** a chunk containing only air
- **When** meshed at step 1 or 2
- **Then** it SHALL contain zero vertices.
#### Scenario: Texture array supplied
- **Given** a texture array and an explicit fallback alpha policy
- **When** startup runs
- **Then** only the custom texture material SHALL be selected.
