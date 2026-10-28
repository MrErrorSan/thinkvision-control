#!/usr/bin/env bash
# Probe-only: NativeAOT publish of the real app from a copy of the repo with core-json-sourcegen.patch applied
# (the unpatched source publishes but crashes at startup: reflection-based System.Text.Json is disabled under AOT).
dotnet publish src/ThinkVisionControl.Avalonia -c Release -r linux-x64 -p:PublishAot=true -p:OptimizationPreference=Size -p:InvariantGlobalization=true -o out/ava-real-aot
