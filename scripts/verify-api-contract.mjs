#!/usr/bin/env node

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import ts from "typescript";
import { parse } from "yaml";

const root = resolve(import.meta.dirname, "..");
const openapiPath = resolve(root, "docs/openapi.yaml");
const sdkPath = resolve(root, "packages/sdk-js/src/index.ts");
const routerPaths = [
  resolve(root, "crates/clotho-api-gateway/src/lib.rs"),
  resolve(root, "crates/clotho-api-gateway/src/secrets.rs"),
];
const methods = new Set(["get", "post", "put", "patch", "delete", "head"]);

function operationKey(method, path) {
  return `${method.toUpperCase()} ${path}`;
}

function pathShape(path) {
  return path.replace(/\{\*?[^}]+\}/g, "{}");
}

function matchingParen(source, open) {
  let depth = 0;
  let quote = null;
  let escaped = false;
  for (let index = open; index < source.length; index += 1) {
    const char = source[index];
    if (quote) {
      if (escaped) escaped = false;
      else if (char === "\\") escaped = true;
      else if (char === quote) quote = null;
      continue;
    }
    if (char === '"' || char === "'") quote = char;
    else if (char === "(") depth += 1;
    else if (char === ")" && --depth === 0) return index;
  }
  throw new Error("unbalanced Axum route registration");
}

function axumOperations() {
  const operations = [];
  for (const file of routerPaths) {
    const source = readFileSync(file, "utf8");
    let cursor = 0;
    while ((cursor = source.indexOf(".route(", cursor)) !== -1) {
      const open = cursor + ".route".length;
      const close = matchingParen(source, open);
      const call = source.slice(open + 1, close);
      const pathMatch = call.match(/^\s*"([^"]+)"\s*,/);
      if (pathMatch) {
        const path = pathMatch[1].replace(/\{\*([^}]+)\}/g, "{$1}");
        const handler = call.slice(pathMatch[0].length);
        for (const match of handler.matchAll(
          /(?:^|[.\s,(])(get|post|put|patch|delete|head)\s*\(/g,
        )) {
          operations.push({ method: match[1], path });
        }
      }
      cursor = close + 1;
    }
  }
  return operations;
}

function openapiOperations(document) {
  const operations = [];
  for (const [path, pathItem] of Object.entries(document.paths ?? {})) {
    for (const [method, operation] of Object.entries(pathItem ?? {})) {
      if (methods.has(method)) {
        operations.push({
          method,
          path,
          operation,
          pathParameters: pathItem.parameters ?? [],
        });
      }
    }
  }
  return operations;
}

function resolveRef(document, ref) {
  assert.match(ref, /^#\//, `external OpenAPI reference is not pinned: ${ref}`);
  return ref
    .slice(2)
    .split("/")
    .map((part) => part.replaceAll("~1", "/").replaceAll("~0", "~"))
    .reduce((value, part) => value?.[part], document);
}

function validateRefs(document) {
  const visit = (value) => {
    if (!value || typeof value !== "object") return;
    if (typeof value.$ref === "string") {
      assert.ok(
        resolveRef(document, value.$ref),
        `unresolved OpenAPI ref: ${value.$ref}`,
      );
    }
    for (const child of Object.values(value)) visit(child);
  };
  visit(document);
}

function expressionName(expression, sourceFile) {
  if (
    ts.isCallExpression(expression) &&
    expression.expression.getText(sourceFile) === "encodeURIComponent"
  ) {
    return expressionName(expression.arguments[0], sourceFile);
  }
  if (ts.isIdentifier(expression)) return expression.text;
  if (ts.isPropertyAccessExpression(expression)) return expression.name.text;
  if (
    ts.isCallExpression(expression) &&
    expression.expression.getText(sourceFile) === "qs"
  ) {
    return "__query__";
  }
  return "param";
}

function staticPath(node, sourceFile) {
  if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) {
    return node.text;
  }
  if (!ts.isTemplateExpression(node)) return null;
  let value = node.head.text;
  for (const span of node.templateSpans) {
    value += `{${expressionName(span.expression, sourceFile)}}${span.literal.text}`;
  }
  return value
    .replace(/\{__query__\}.*$/, "")
    .replace(/\{q\}$/, "")
    .split("?")[0];
}

function sdkOperations() {
  const source = readFileSync(sdkPath, "utf8");
  const sourceFile = ts.createSourceFile(
    sdkPath,
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TS,
  );
  const operations = [];
  const interfaces = new Map();
  const interfaceNames = new Set(
    sourceFile.statements
      .filter(ts.isInterfaceDeclaration)
      .map((statement) => statement.name.text),
  );
  const aliases = new Map(
    sourceFile.statements
      .filter(ts.isTypeAliasDeclaration)
      .map((statement) => [statement.name.text, statement.type]),
  );

  function typeCategory(type, seen = new Set()) {
    if (!type) return "unknown";
    if (type.kind === ts.SyntaxKind.StringKeyword) return "string";
    if (type.kind === ts.SyntaxKind.NumberKeyword) return "number";
    if (type.kind === ts.SyntaxKind.BooleanKeyword) return "boolean";
    if (
      type.kind === ts.SyntaxKind.AnyKeyword ||
      type.kind === ts.SyntaxKind.UnknownKeyword
    ) {
      return "unknown";
    }
    if (ts.isArrayTypeNode(type) || ts.isTupleTypeNode(type)) return "array";
    if (ts.isTypeLiteralNode(type)) return "object";
    if (ts.isUnionTypeNode(type)) {
      const categories = [
        ...new Set(
          type.types
            .filter(
              (part) =>
                part.kind !== ts.SyntaxKind.UndefinedKeyword &&
                !(
                  ts.isLiteralTypeNode(part) &&
                  part.literal.kind === ts.SyntaxKind.NullKeyword
                ),
            )
            .map((part) => typeCategory(part, seen)),
        ),
      ];
      if (categories.includes("string")) return "string";
      if (categories.includes("number")) return "number";
      return categories.length === 1 ? categories[0] : categories.join("|");
    }
    if (ts.isLiteralTypeNode(type)) {
      return ts.isStringLiteral(type.literal) ? "string" : "number";
    }
    if (ts.isTypeReferenceNode(type)) {
      const name = type.typeName.getText(sourceFile);
      if (name === "Array" || name === "ReadonlyArray") return "array";
      if (name === "Record" || interfaceNames.has(name)) return "object";
      if (aliases.has(name) && !seen.has(name)) {
        return typeCategory(aliases.get(name), new Set([...seen, name]));
      }
      return "object";
    }
    return "unknown";
  }

  for (const statement of sourceFile.statements) {
    if (!ts.isInterfaceDeclaration(statement)) continue;
    const properties = statement.members
      .filter(ts.isPropertySignature)
      .map((member) => member.name?.getText(sourceFile).replaceAll(/["']/g, ""))
      .filter(Boolean)
      .sort();
    const required = statement.members
      .filter(
        (member) => ts.isPropertySignature(member) && !member.questionToken,
      )
      .map((member) => member.name?.getText(sourceFile).replaceAll(/["']/g, ""))
      .filter(Boolean)
      .sort();
    const types = Object.fromEntries(
      statement.members
        .filter(ts.isPropertySignature)
        .filter((member) => member.name)
        .map((member) => [
          member.name.getText(sourceFile).replaceAll(/["']/g, ""),
          typeCategory(member.type),
        ]),
    );
    interfaces.set(statement.name.text, { properties, required, types });
  }

  function walk(node, methodName = null) {
    if (ts.isMethodDeclaration(node) && node.name)
      methodName = node.name.getText(sourceFile);
    if (
      ts.isCallExpression(node) &&
      ts.isPropertyAccessExpression(node.expression) &&
      node.expression.expression.kind === ts.SyntaxKind.ThisKeyword &&
      node.expression.name.text === "request"
    ) {
      const path = staticPath(node.arguments[0], sourceFile);
      if (path) {
        let method = "get";
        const init = node.arguments[1];
        if (init && ts.isObjectLiteralExpression(init)) {
          for (const property of init.properties) {
            if (
              ts.isPropertyAssignment(property) &&
              property.name.getText(sourceFile) === "method" &&
              ts.isStringLiteral(property.initializer)
            ) {
              method = property.initializer.text.toLowerCase();
            }
          }
        }
        operations.push({ method, path, sdkMethod: methodName });
      }
    }
    ts.forEachChild(node, (child) => walk(child, methodName));
  }
  walk(sourceFile);
  return { operations, interfaces };
}

const document = parse(readFileSync(openapiPath, "utf8"));
assert.match(document.openapi, /^3\./, "docs/openapi.yaml must be OpenAPI 3.x");
validateRefs(document);

const contract = document["x-clotho-contract"];
assert.ok(contract, "OpenAPI must declare x-clotho-contract defaults");
assert.equal(contract.stability, "alpha");
assert.equal(contract.errorResponse, "#/components/responses/Error");
assert.ok(resolveRef(document, contract.errorResponse));

const openapi = openapiOperations(document);
const operationIds = new Set();
for (const { method, path, operation, pathParameters } of openapi) {
  assert.ok(
    operation.operationId,
    `${method.toUpperCase()} ${path} has no operationId`,
  );
  assert.ok(
    !operationIds.has(operation.operationId),
    `duplicate operationId: ${operation.operationId}`,
  );
  operationIds.add(operation.operationId);
  assert.ok(operation.summary, `${operation.operationId} has no summary`);

  const stability = operation["x-clotho-stability"] ?? contract.stability;
  assert.ok(
    ["alpha", "beta", "stable", "deprecated"].includes(stability),
    `${operation.operationId} has invalid stability`,
  );
  const auth =
    operation["x-clotho-auth"] ??
    (operation.security ? "human-bearer" : contract.auth);
  assert.ok(
    ["public", "bootstrap-or-bearer", "human-bearer", "webhook-hmac"].includes(
      auth,
    ),
    `${operation.operationId} has invalid auth metadata`,
  );

  if (["post", "put", "patch"].includes(method)) {
    assert.ok(
      operation.requestBody,
      `${operation.operationId} has no requestBody schema`,
    );
  }
  const successes = Object.entries(operation.responses ?? {}).filter(
    ([status]) => /^2\d\d$/.test(status),
  );
  assert.ok(
    successes.length > 0,
    `${operation.operationId} has no explicit success response`,
  );
  for (const [status, responseOrRef] of successes) {
    const response = responseOrRef.$ref
      ? resolveRef(document, responseOrRef.$ref)
      : responseOrRef;
    if (method === "head" || status === "204") continue;
    assert.ok(
      response.content,
      `${operation.operationId} ${status} has no response content`,
    );
    assert.ok(
      Object.values(response.content).some((media) => media.schema),
      `${operation.operationId} ${status} has no response schema`,
    );
  }

  const declaredParams = new Set(
    [...pathParameters, ...(operation.parameters ?? [])]
      .map((parameter) =>
        parameter.$ref ? resolveRef(document, parameter.$ref) : parameter,
      )
      .filter((parameter) => parameter.in === "path")
      .map((parameter) => parameter.name),
  );
  for (const name of [...path.matchAll(/\{([^}]+)\}/g)].map(
    (match) => match[1],
  )) {
    assert.ok(
      declaredParams.has(name),
      `${operation.operationId} does not declare path parameter ${name}`,
    );
  }
}

const axum = axumOperations();
const openapiKeys = new Set(
  openapi.map(({ method, path }) => operationKey(method, path)),
);
const axumKeys = new Set(
  axum.map(({ method, path }) => operationKey(method, path)),
);
assert.deepEqual(
  [...openapiKeys].sort(),
  [...axumKeys].sort(),
  "Axum method/path inventory differs from OpenAPI",
);

const sdk = sdkOperations();
for (const operation of sdk.operations) {
  const shape = operationKey(operation.method, pathShape(operation.path));
  assert.ok(
    openapi.some(
      (candidate) =>
        operationKey(candidate.method, pathShape(candidate.path)) === shape,
    ),
    `SDK method ${operation.sdkMethod} calls undocumented ${operationKey(operation.method, operation.path)}`,
  );
}

const manualSdkCoverage = new Map([
  ["GET /api/v1/repos/{}/releases/{}/resolve/{}", "downloadReleaseFile"],
  ["HEAD /api/v1/repos/{}/releases/{}/resolve/{}", "downloadReleaseFile"],
]);
const sdkShapes = new Set(
  sdk.operations.map(({ method, path }) =>
    operationKey(method, pathShape(path)),
  ),
);
const missingSdk = openapi
  .filter(
    ({ path }) =>
      path === "/healthz" ||
      (path.startsWith("/api/v1/") && !path.startsWith("/api/v1/webhooks/")),
  )
  .filter(({ method, path }) => {
    const shape = operationKey(method, pathShape(path));
    return !sdkShapes.has(shape) && !manualSdkCoverage.has(shape);
  })
  .map(
    ({ method, path, operation }) =>
      `${operationKey(method, path)} (${operation.operationId})`,
  );
assert.deepEqual(
  missingSdk,
  [],
  `canonical REST operations missing SDK coverage:\n${missingSdk.join("\n")}`,
);

function schemaCategory(schema) {
  if (!schema) return "unknown";
  if (schema.$ref) return schemaCategory(resolveRef(document, schema.$ref));
  if (schema.type === "integer" || schema.type === "number") return "number";
  return schema.type ?? "unknown";
}

for (const [name, schema] of Object.entries(
  document.components?.schemas ?? {},
)) {
  const sdkInterface = sdk.interfaces.get(name);
  if (!sdkInterface || !schema.properties) continue;
  assert.deepEqual(
    Object.keys(schema.properties).sort(),
    sdkInterface.properties,
    `SDK interface ${name} properties differ from OpenAPI`,
  );
  assert.deepEqual(
    [...(schema.required ?? [])].sort(),
    sdkInterface.required,
    `SDK interface ${name} required fields differ from OpenAPI`,
  );
  for (const [property, sdkType] of Object.entries(sdkInterface.types)) {
    const openapiType = schemaCategory(schema.properties[property]);
    if (sdkType === "unknown" || openapiType === "unknown") continue;
    assert.equal(
      openapiType,
      sdkType,
      `SDK interface ${name}.${property} type differs from OpenAPI`,
    );
  }
}

if (process.argv.includes("--json")) {
  const inventory = openapi
    .map(({ method, path, operation }) => {
      const shape = operationKey(method, pathShape(path));
      return {
        method: method.toUpperCase(),
        path,
        operation_id: operation.operationId,
        stability: operation["x-clotho-stability"] ?? contract.stability,
        auth:
          operation["x-clotho-auth"] ??
          (operation.security ? "human-bearer" : contract.auth),
        sdk_methods: [
          ...new Set([
            ...sdk.operations
              .filter(
                (candidate) =>
                  operationKey(candidate.method, pathShape(candidate.path)) ===
                  shape,
              )
              .map((candidate) => candidate.sdkMethod),
            ...(manualSdkCoverage.has(shape)
              ? [manualSdkCoverage.get(shape)]
              : []),
          ]),
        ].sort(),
      };
    })
    .sort((left, right) =>
      `${left.path} ${left.method}`.localeCompare(
        `${right.path} ${right.method}`,
      ),
    );
  process.stdout.write(
    `${JSON.stringify(
      {
        version: 1,
        counts: {
          openapi_operations: openapi.length,
          axum_operations: axum.length,
          sdk_calls: sdk.operations.length,
          sdk_interfaces: sdk.interfaces.size,
        },
        operations: inventory,
      },
      null,
      2,
    )}\n`,
  );
} else {
  process.stdout.write(
    `API contract verified: ${openapi.length} OpenAPI operations, ${axum.length} Axum operations, ${sdk.operations.length} SDK calls, ${sdk.interfaces.size} SDK interfaces.\n`,
  );
}
