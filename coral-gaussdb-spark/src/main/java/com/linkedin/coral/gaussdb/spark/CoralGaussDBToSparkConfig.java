/**
 * Copyright 2024-2026 LinkedIn Corporation. All rights reserved.
 * Licensed under the BSD-2 Clause license.
 * See LICENSE in the project root for license information.
 */
package com.linkedin.coral.gaussdb.spark;

/**
 * Configuration knobs for {@link CoralGaussDBToSpark}. Immutable; built via
 * {@link Builder}.
 *
 * <p>v1 fields:
 * <ul>
 *   <li>{@code defaultDatabase} — when GaussDB input uses 3-part names
 *       {@code database.schema.table}, this is the expected top-level database.
 *       Inputs whose database segment does not match are rejected. Matches the
 *       user-approved mapping policy: {@code schema → Hive db}, top-level
 *       database is a configured constant.</li>
 *   <li>{@code passthroughUnknownFunctions} — default {@code false} (hard-fail
 *       on unknown functions per plan). Reserved for future relaxation.</li>
 * </ul>
 *
 * <p>Kept deliberately small in v1 — additional knobs (nullOrder defaults,
 * numeric-precision policy, hint-handling mode) will be added as Stage 2/3 needs
 * dictate, without breaking the existing constructor calls.
 */
public final class CoralGaussDBToSparkConfig {

  private final String defaultDatabase;
  private final boolean passthroughUnknownFunctions;

  private CoralGaussDBToSparkConfig(Builder b) {
    this.defaultDatabase = b.defaultDatabase;
    this.passthroughUnknownFunctions = b.passthroughUnknownFunctions;
  }

  public String getDefaultDatabase() {
    return defaultDatabase;
  }

  public boolean isPassthroughUnknownFunctions() {
    return passthroughUnknownFunctions;
  }

  public static Builder builder() {
    return new Builder();
  }

  public static final class Builder {
    private String defaultDatabase;
    private boolean passthroughUnknownFunctions = false;

    public Builder defaultDatabase(String db) {
      this.defaultDatabase = db;
      return this;
    }

    public Builder passthroughUnknownFunctions(boolean v) {
      this.passthroughUnknownFunctions = v;
      return this;
    }

    public CoralGaussDBToSparkConfig build() {
      return new CoralGaussDBToSparkConfig(this);
    }
  }
}
