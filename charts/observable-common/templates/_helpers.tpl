{{/*
Common labels for all Observable resources.
Context must contain .Chart and .Release.
*/}}
{{- define "observable-common.labels" -}}
helm.sh/chart: {{ .Chart.Name }}-{{ .Chart.Version | replace "+" "_" }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/part-of: observable
{{- end }}

{{/*
Selector labels for a specific service component.
Context must contain .name and .Release.
*/}}
{{- define "observable-common.selectorLabels" -}}
app.kubernetes.io/name: {{ .name }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Full image reference for one component (Phase 7, docs/component-decomposition.md).
Context keys:
  Values — root .Values (for global.image.registry)
  image  — the component's .Values.services.<name>.image ({repository, tag})
  tag    — optional tag override (e.g. a canary tag); wins over image.tag
Renders [registry/]repository:tag. Fails when no tag is set.
*/}}
{{- define "observable-common.componentImage" -}}
{{- $registry := trimSuffix "/" (.Values.global.image.registry | default "") -}}
{{- $repository := required "services.<name>.image.repository is required" .image.repository -}}
{{- $tag := required (printf "an image tag is required for %s" $repository) (.tag | default .image.tag) -}}
{{- if $registry -}}
{{ $registry }}/{{ $repository }}:{{ $tag }}
{{- else -}}
{{ $repository }}:{{ $tag }}
{{- end -}}
{{- end }}

{{/*
Browser-facing HTTP origin from a domain and port.
Omits the explicit port for the default HTTP/HTTPS ports so URLs stay stable
when the public listener is exposed on plain localhost.
Context must contain .domain and .port.
*/}}
{{- define "observable-common.httpOrigin" -}}
{{- $port := int .port -}}
{{- if or (eq $port 80) (eq $port 443) -}}
http://{{ .domain }}
{{- else -}}
http://{{ .domain }}:{{ $port }}
{{- end -}}
{{- end }}
