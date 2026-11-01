{{/*
Helpers for secret hardening and credential injection across Observable platform services.
*/}}

{{/*
Renders the PostgreSQL environment entry (either plaintext value or valueFrom.secretKeyRef).
Context keys:
  Values        - root .Values
  service       - component service values (e.g. .Values.services.authService)
  scopedEnvName - scoped environment variable name (e.g. "AUTH_DATABASE_URL")
*/}}
{{- define "observable.postgresEnv" -}}
{{- if and .service (or .service.existingSecret .service.databaseUrl) -}}
  {{- $scopedEnv := .scopedEnvName | default "DATABASE_URL" -}}
  {{- if .service.existingSecret -}}
name: {{ $scopedEnv }}
valueFrom:
  secretKeyRef:
    name: {{ .service.existingSecret }}
    key: {{ .service.existingSecretKey | default $scopedEnv }}
  {{- else -}}
name: {{ $scopedEnv }}
value: {{ .service.databaseUrl | quote }}
  {{- end -}}
{{- else if .Values.postgres.existingSecret -}}
name: DATABASE_URL
valueFrom:
  secretKeyRef:
    name: {{ .Values.postgres.existingSecret }}
    key: {{ .Values.postgres.existingSecretKey | default "DATABASE_URL" }}
{{- else -}}
name: DATABASE_URL
value: {{ .Values.postgres.url | quote }}
{{- end -}}
{{- end -}}

{{/*
Renders the ClickHouse password environment entry.
Context: root .Values
*/}}
{{- define "observable.clickhousePasswordEnv" -}}
{{- if .Values.clickhouse.existingSecret -}}
name: CLICKHOUSE_PASSWORD
valueFrom:
  secretKeyRef:
    name: {{ .Values.clickhouse.existingSecret }}
    key: {{ .Values.clickhouse.existingSecretKey | default "CLICKHOUSE_PASSWORD" }}
{{- else -}}
name: CLICKHOUSE_PASSWORD
value: {{ .Values.clickhouse.password | quote }}
{{- end -}}
{{- end -}}

{{/*
Renders the internal service token environment entry.
Context: root .Values
*/}}
{{- define "observable.internalServiceTokenEnv" -}}
{{- if .Values.internalServiceTokenSecret -}}
name: INTERNAL_SERVICE_TOKEN
valueFrom:
  secretKeyRef:
    name: {{ .Values.internalServiceTokenSecret }}
    key: {{ .Values.internalServiceTokenSecretKey | default "INTERNAL_SERVICE_TOKEN" }}
{{- else -}}
name: INTERNAL_SERVICE_TOKEN
value: {{ .Values.internalServiceToken | quote }}
{{- end -}}
{{- end -}}

{{/*
Renders the session secret environment entry for auth-service.
Context: root .Values
*/}}
{{- define "observable.sessionSecretEnv" -}}
{{- if .Values.identity.existingSecret -}}
name: SESSION_SECRET
valueFrom:
  secretKeyRef:
    name: {{ .Values.identity.existingSecret }}
    key: {{ .Values.identity.existingSecretKey | default "SESSION_SECRET" }}
{{- else -}}
name: SESSION_SECRET
value: {{ .Values.identity.sessionSecret | quote }}
{{- end -}}
{{- end -}}

{{/*
Renders the bearer token environment entry for selfObservability.
Context: root .Values
*/}}
{{- define "observable.selfObservabilityBearerTokenEnv" -}}
{{- if .Values.selfObservability.existingSecret -}}
name: OBSERVABLE_SELF_OBSERVABILITY_BEARER_TOKEN
valueFrom:
  secretKeyRef:
    name: {{ .Values.selfObservability.existingSecret }}
    key: {{ .Values.selfObservability.existingSecretKey | default "OBSERVABLE_SELF_OBSERVABILITY_BEARER_TOKEN" }}
{{- else -}}
name: OBSERVABLE_SELF_OBSERVABILITY_BEARER_TOKEN
value: {{ .Values.selfObservability.bearerToken | quote }}
{{- end -}}
{{- end -}}
