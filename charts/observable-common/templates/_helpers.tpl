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
Full image reference from global.image values.
Context must contain .Values.global.image.{repository,tag}.
*/}}
{{- define "observable-common.image" -}}
{{ .Values.global.image.repository }}:{{ .Values.global.image.tag | default "latest" }}
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
